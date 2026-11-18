#!/usr/bin/env python3
"""List the face names of the LightCraft library open in the running app, propose merges, apply an agreed plan.

Talks to the app's control channel (JSON lines on 127.0.0.1:PORT). Standard library only.

  face_names.py list    [--port 18731] [--json OUT]   every face name, with the number of photos it is on
  face_names.py suggest [--port 18731] [--json OUT]   groups of names that look like one person (to review)
  face_names.py apply PLAN.json [--port 18731]         merge, one faces.renamePerson {from, to, merge: true} each

`list` and `suggest` change nothing. `apply` reads a plan the user agreed to: [{"from": "...", "to": "..."}, ...].
Each merge is its own undo step, stored in the catalog only (no sidecar or file is written).
"""
import argparse, collections, json, socket, sys, time, unicodedata

RETRY = ("drew no frame", "was not run")


class App:
    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=120)
        self.f = self.sock.makefile("rw", encoding="utf-8")
        self.n = 0

    def run(self, command, params=None):
        for _ in range(30):
            self.n += 1
            req = {"id": self.n, "method": "engine.execute", "params": {"command": command, "params": params or {}}}
            self.f.write(json.dumps(req, ensure_ascii=False) + "\n"); self.f.flush()
            r = json.loads(self.f.readline())
            if r.get("ok"):
                return r.get("result")
            err = str(r.get("error"))
            if not any(s in err for s in RETRY):
                raise RuntimeError("%s: %s" % (command, err))
            time.sleep(10)
        raise RuntimeError("%s: the app stayed busy" % command)


def names(app):
    """{name: photos}: every distinct face name (trimmed; capitalisation kept as first seen) and its photo count."""
    ids, off = [], 0
    while True:
        q = app.run("catalog.query", {"filter": {}, "offset": off, "limit": 5000})
        ids += [p["id"] for p in q["photos"]]; off += len(q["photos"])
        if not q["photos"] or off >= q["total"]:
            break
    count = collections.Counter()
    for i in ids:
        seen = set()
        for r in (app.run("photo.inspect", {"id": i}).get("meta") or {}).get("regions") or []:
            n = (r.get("name") or "").strip()
            if n and n not in seen:
                seen.add(n); count[n] += 1
    return dict(count)


def fold(s):
    s = unicodedata.normalize("NFKD", s)
    return " ".join("".join(c for c in s if not unicodedata.combining(c)).lower().replace("-", " ").split())


def close(a, b):
    """At most one letter added, removed or changed (Rafiian / Raffiian, Moran / Morian)."""
    if abs(len(a) - len(b)) > 1:
        return False
    if len(a) > len(b):
        a, b = b, a
    i = 0
    while i < len(a) and a[i] == b[i]:
        i += 1
    return a[i:] == b[i + 1:] or (len(a) == len(b) and a[i + 1:] == b[i + 1:])


def suggest(count):
    """Groups to review, each {kind, names: {name: photos}}. A heuristic only: the user decides every merge."""
    out, used = [], set()
    by_fold = collections.defaultdict(list)
    for n in count:
        by_fold[fold(n)].append(n)
    for group in by_fold.values():
        if len(group) > 1:  # capitals, accents, hyphens or spaces only
            out.append({"kind": "same letters", "names": {n: count[n] for n in group}}); used.update(group)
    for n in count:  # stray spaces: renaming to the trimmed name merges nothing else
        if " ".join(n.split()) != n and len(by_fold[fold(n)]) == 1:
            out.append({"kind": "spacing", "names": {n: count[n]}})
    full = sorted(count)
    for n in full:
        for m in full:
            if n < m and n not in used and m not in used and " " in n and close(fold(n), fold(m)):
                out.append({"kind": "spelling", "names": {n: count[n], m: count[m]}})
    for n in full:
        f = fold(n)
        longer = [m for m in full if fold(m) != f and (fold(m).startswith(f + " ") or (" " in f and fold(m).startswith(f)))]
        if longer:
            kind = "first name, one full name" if len(longer) == 1 and " " not in f else (
                "name, a longer one" if len(longer) == 1 else "ambiguous: several longer names")
            out.append({"kind": kind, "names": {n: count[n], **{m: count[m] for m in longer}}})
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("action", choices=["list", "suggest", "apply"])
    ap.add_argument("plan", nargs="?")
    ap.add_argument("--port", type=int, default=18731)
    ap.add_argument("--json")
    a = ap.parse_args()
    app = App(a.port)
    info = app.run("library.info")
    print("library: %s (%d photos)" % (info.get("path"), info.get("photos", 0)), file=sys.stderr)
    if a.action == "apply":
        if not a.plan:
            sys.exit("apply needs PLAN.json")
        for m in json.load(open(a.plan, encoding="utf-8")):
            try:
                r = app.run("faces.renamePerson", {"from": m["from"], "to": m["to"], "merge": True})
                print("%s -> %s: %s faces on %s photos" % (m["from"], m["to"], r.get("faces"), r.get("photos")))
            except RuntimeError as e:
                print("%s -> %s: FAILED %s" % (m["from"], m["to"], e))
        return
    count = names(app)
    result = count if a.action == "list" else suggest(count)
    if a.json:
        json.dump(result, open(a.json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    if a.action == "list":
        print("%d names" % len(count))
        for n in sorted(count, key=str.lower):
            print("%5d  %s" % (count[n], n))
    else:
        for g in result:
            print("%-28s %s" % (g["kind"], " | ".join("%s (%d)" % kv for kv in g["names"].items())))


if __name__ == "__main__":
    main()

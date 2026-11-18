---
name: merge-face-names
description: Find face names in the open LightCraft library that belong to one person (a first name and a full name, capitals, spelling, maiden and married names, stray spaces), propose merges, check them with pictures-library's curated people, and on the user's OK merge them with Rename Person (faces.renamePerson, merge). Use when the user asks to clean up, merge, deduplicate or harmonise people or face names in a LightCraft library, for instance after seeding it with names from another library.
---

# merge-face-names

Merges face names that mean one person in the library open in the running LightCraft app (control port 18731 by
default). Helper: `face_names.py` in this skill's base directory (in the repository,
`claude-plugins/slg-ltc/skills/merge-face-names/`). It needs only Python's standard library.

LightCraft has no command that lists face names, so `face_names.py list` reads them photo by photo (`catalog.query`
then `photo.inspect`), which takes a minute or two on 5,000 photos. It changes nothing. The merges themselves are the
app's own Rename Person (`faces.renamePerson {from, to, merge: true}`).

## Rules

- **Names only.** A merge changes face names in the library's catalog. No photo, file, sidecar or NAS folder is written.
- **The user decides every merge.** The script's groups are a heuristic. A first name shared by several people
  (Nicolas, Albane, Catherine…) is never merged as a whole: leave it, or settle it face by face on the person's page.
- **The curated spellings win.** When pictures-library is reachable (ListAgents: the session named "Document reading
  request" or pictures-library), send it the proposal and use the canonical names it returns.
- **One merge, one undo step.** Each can be undone with Cmd+Z in the app.

## Steps

1. **Which library.** Say which library is open (the script prints it). If it's not the one the user means, stop.
2. **Propose.** Run `face_names.py suggest --json <work folder>/tmp/<library>_suggest.json` from
   `/Volumes/PhotoEditing/lightcraft-work/tmp`. Present the result in three groups:
   - **very likely the same person**: capitals, accents, hyphens or spaces only; a one-letter spelling difference; a
     first name or a shorter name with exactly one longer match;
   - **to decide**: several possible matches, or a different family name (maiden and married names);
   - **leave apart**: first names that match several people.

   Give the photo counts, and for each group the name to keep (usually the fullest, most used one).
3. **Check with pictures-library** if it's reachable: send the groups and the open questions. Ask which merges are
   wrong, which canonical spelling each kept name has, and which open questions the curated stores settle. Fold its
   answers in. Where its stores disagree among themselves (accents, one spelling against another), ask the user.
4. **Agree the plan with the user.** Write it as `[{"from": "...", "to": "..."}]` to
   `<work folder>/tmp/<library>_merge_plan.json`, one entry per name that goes away. A kept name that should change
   spelling is an entry too (`"from": "Damien Ramos", "to": "Damian Ramos"`). Show the plan; apply only after the
   user's OK.
5. **Apply.** Run `face_names.py apply <plan>`. It prints the faces and photos each merge touched, and FAILED with
   the reason for one that did not apply (a name that no longer exists, for instance).
6. **Check.** Run `face_names.py list` again. Report how many names are left, and confirm that every "from" is gone
   and every "to" exists.

## Options

- `--port N`: the app's control port (default 18731).
- `--json OUT`: also write the list or the groups as JSON.

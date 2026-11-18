# Driving LightCraft from an agent (fork notes)

Fork-only notes (alemaxdesign/lightcraft, branch `alemax-main`). They record what we learned driving the desktop app
from Claude Code over its control channel and MCP server. Nothing here is meant for upstream yet: open issues or PRs
on storytold/lightcraft only when the user decides.

## Setup

- **Build** from `alemax-main` with HEIC support:
  `cargo build --release -p lightcraft -p lightcraft-cli --features lightcraft/heif,lightcraft-cli/heif`.
- **Launch detached**, so the app outlives the agent session that started it. An app started as a session's
  background task is killed when that session ends, even mid-import:
  ```
  nohup ./target/release/lightcraft --library "<library folder>" --no-demo --control 18731 >> <log> 2>&1 < /dev/null & disown
  nohup caffeinate -i -m -s -w <app pid> >/dev/null 2>&1 < /dev/null & disown   # long jobs survive a locked screen
  ```
- **Quit cleanly** with the control request `{"method":"app.quit"}` (the same as Cmd+Q) once `library.info` shows
  `unsavedOps: 0`. The catalog is compacted on quit.
- **MCP server** for this repository only (local scope, not committed):
  `claude mcp add lightcraft --scope local -- <repo>/target/release/lightcraft-cli mcp --connect 127.0.0.1:18731`.
  Its tools (`ui_inspect`, `run_command`, `import`, `screenshot`, …) load when a session starts: after adding the
  server, start a new session (`claude --continue`); `/reload-plugins` does not load them. The server connects to
  whatever app listens on the port, so start the app first.

## MCP or scripts

Both go through the same control channel to the same commands; neither runs anything the other can't.

| | MCP tools | Ad hoc scripts |
|---|---|---|
| Visibility | each call shows in Claude Code; the user can approve or refuse it | hidden inside the script |
| Docs | tool descriptions and `list_commands` | whatever the script says |
| Limits | one call at a time, each answered within 60 s | loops, retries, polling |
| Big results | land in the agent's context (`ui_inspect` is large) | can be summarized first |

**Use MCP and let LightCraft do the work.** A script that walks the NAS, cuts folders into chunks or feeds files one
batch at a time is doing the app's job: don't. If the app lacks something, write it down (below) rather than
scripting around it.

## Long commands block the app

The control channel runs each command on the app's main thread, between frames. A command that takes long freezes
the window and the channel:

- The window doesn't redraw (it looks empty or stuck) until the command ends.
- The caller gets `timeout` after 60 s, but **the command keeps running** to the end.
- Every other request meanwhile is answered "the app drew no frame for 5 s, so the request was not run": it did not
  run, so retrying is safe. You can't inspect, screenshot or cancel until the command ends.
- Without the channel, watch the process (`ps`) and the library's `catalog.log`: an import writes its records when
  it is committed.

| Runs on the main thread (blocks) | Runs in the background (window stays live, row in the activity stack) |
|---|---|
| `library.import` (MCP `import`) | the import review: `ui.dropFiles {paths:[folder]}`, then Import |
| `library.importPreview` | the Synchronize Folder dialog (Folders panel) |
| `folder.scanChanges`, `folder.synchronize` | `library.importLightroom` (poll with `{status: true}`) |
| `buildPreviews` with `wait: true` | `library.buildPreviews {background: true}` (poll `library.previewProgress`) |

So for a big folder (thousands of files on a NAS), prefer the background flows. The MCP server has no tool for
`ui.dropFiles` (it is a control-channel method, not an engine command); until it does, the user can drop the folder
on the window.

## Gotchas

- **`catalog.query` without `filter` lists the current view only** (e.g. 2 photos when an album is on screen). Pass
  `filter: {}` for the whole catalog.
- **`library.buildPreviews` without `ids` walks the whole view**, even photos that already have previews: pass the
  new photos' ids. A build the UI asks for (scrolling the grid) replaces a background build, so `previewProgress`
  may then describe the UI's build.
- **`folder.scanChanges` / `folder.synchronize` refuse a folder the library has no photo from** ("no photo in the
  library was imported from it"): a new folder (a new year on the NAS) is an import, not a sync.
- **Local (browsed) photos** show only in folder views: use `library.source {kind: "all"}` plus a folder filter.
- **Files with a photo extension that aren't photos** (QuickTime movies named `.HEIC`) fail as "unrecognized file
  format"; videos (`.mov`, `.mp4`, …) are left out or unreadable. Report them; don't retry.
- **Lightroom import**: smart collections become empty albums (the warning says otherwise); person keyword types,
  catalog-only GPS, Lightroom face tables and stacks are not read.
- **Read-only sources**: keep `autoWrite` off and `readMetadata: false` / `removeMissing: false` when syncing, so
  nothing is written to the NAS or removed from the library without a decision.

## Possible upstream issues (not filed)

- A background, pollable `library.import` (like `library.importLightroom {status: true}`), or an MCP tool that opens
  the import review for a folder.
- The smart-collection warning, and the Lightroom data not read (above).
- A read-only way to open a library from `lightcraft-cli` (today `run` / `mcp --library` take the lock and save).

---
name: fork-sync
description: Sync the alemaxdesign/lightcraft fork with storytold/lightcraft. main mirrors upstream (fast-forward only); alemax-main merges upstream in and takes our work only through squash-merged PRs; PR branches claude/<topic> are rebased onto alemax-main. Claude sits on alemax-main and creates one PR branch per requested change. Use when the user asks to sync, update or rebase the fork, pull upstream changes, check whether upstream moved, or open or merge a PR of our work.
---

# fork-sync

Three layers (the same strategy as PhotoCraft; rules in `CLAUDE.local.md`):

| Branch | Role | Moves by |
|---|---|---|
| `main` | exact mirror of `upstream/main` (storytold) | fast-forward only |
| `alemax-main` | the fork's integrated version: upstream + our accepted work | squash-merged PRs on GitHub; `upstream/main` merged in (never rebased, never force-pushed) |
| `claude/<topic>` | one PR branch per requested change | commits by Claude; rebased onto `alemax-main`; deleted once merged |

Script: `fork-sync.sh` in this skill's base directory (in the repository,
`claude-plugins/slg-ltc/skills/fork-sync/`), run from the repository root. Tests:
`test-fork-sync.sh [WORKDIR]`, next to it (throwaway repos; default `$TMPDIR/lightcraft-forksync-test`;
pass a folder in the session's scratchpad). Both are copied from PhotoCraft's skill; keep the two in step.

## Sync

1. Preview: `fork-sync.sh --dry-run --push`.
2. If it **refuses to sync main** (origin/main or local main has commits upstream lacks) or **refuses to sync
   alemax-main** (local commits that did not come through a PR), show the user the list and stop. Fixing it means
   moving commits to a `claude/<topic>` branch and resetting a branch: ask first.
3. Otherwise run `fork-sync.sh --push`. Pushing is outward-facing: run it when the user asked to sync or push.
4. A **CONFLICT merging upstream into alemax-main** or **rebasing a PR branch** is aborted with nothing changed.
   Report it, and resolve by hand only with the user.
5. Report:
   - the commits main gained;
   - whether alemax-main merged upstream;
   - how many own commits each PR branch carries;
   - any branch reported **LANDED** (its PR was merged). Offer to delete it
     (`git branch -d <b> && git push origin --delete <b>`) and switch back to alemax-main.
6. To pick up changes in the running app, rebuild from `alemax-main`
   (`cargo build --release -p lightcraft -p lightcraft-cli --features lightcraft/heif,lightcraft-cli/heif`) and offer
   to restart it. Quit LightCraft cleanly first (control channel `{"method":"app.quit"}`, the same as Cmd+Q), then
   relaunch it on the same `--library` and `--control` port.

## A requested change = one PR (claude/<topic> into alemax-main)

0. Default state: on `alemax-main`, clean, synced. Never commit on it.
1. Sync (`fork-sync.sh --push`), then `git switch -c claude/<topic> alemax-main`, commit, and
   `git push -u origin claude/<topic>`.
2. `gh pr create --repo alemaxdesign/lightcraft --base alemax-main --head claude/<topic> --title "<type>(<scope>): …" --body …`
   - Always pass `--repo` and `--base`: the fork's default branch is main, and a PR must never target main or upstream.
   - No agent attribution in the body (house rule 15).
3. The user reviews. Merge only when they say so: `gh pr merge <n> --repo alemaxdesign/lightcraft --squash`. Never
   `--merge` or `--rebase`; keep the branch (no `--delete-branch`).
4. Sync again: the script sees the squash commit and reports the branch LANDED. Delete it locally and on origin,
   then `git switch alemax-main`.

Something for storytold goes on a separate branch cut from `upstream/main` with only those commits, never from
alemax-main or a claude/<topic> branch (they carry fork-only changes). Open a PR to storytold only when the user
decides.

## Options

- `BRANCH...`: only these working branches (default: every local branch but main and alemax-main)
- `--push`: push main and alemax-main (plain push, so a non-fast-forward is refused) and the rebased working
  branches that exist on origin (`--force-with-lease`)
- `--dry-run`: fetch, then only describe
- env `FORK_SYNC_INTEGRATION`: integration branch name (default `alemax-main`)

## Guarantees

- main is never merged into, rebased or force-pushed; alemax-main is never rebased or force-pushed.
- Local commits on alemax-main that did not come from a PR or upstream stop the script.
- Conflicts are aborted at once, so nothing is left half-merged or half-rebased, and the starting branch is restored.
- Every push's exit status is checked.
- The tree must have no uncommitted changes to tracked files.

---
name: hdd-change
description: Start a human-driven-development (HDD) change — set up the change's slug, directory, branch and worktree, then help the user write the change into the project's `_intent/` files by answering questions, suggesting edits and catching mistakes, until the intent is ready to implement. Use when the user wants to make, start or specify a change to software in a repo with an `_intent/` directory, or says "hdd change".
---

# HDD change

**Goal:** help the user specify a change to the software by modifying the intent.

In HDD, humans write the intent. You help them think, catch mistakes and get the change ready for implementation. Suggest changes for the user to make; do not write intent files yourself unless the user explicitly tells you to.

## Conventions

These are shared by all HDD skills.

- `<repo>` is the root of the primary checkout of the repo.
- `<change-slug>` is a short kebab-case identifier for the change, e.g. `add-profile-page`, `rework-navbar`, `fix-loading-animation`, `redact-secrets`.
- `<change-id>` is the current local timestamp plus the slug: `YYYY-MM-DD-hhmm-<change-slug>`, e.g. `2026-11-07-0030-intent-skills`.
- `<change-dir>` is `<repo>/.changes/<change-id>`. It holds ephemeral content for the change, is gitignored and is managed by agents.
- `<change-branch>` is `change/<change-slug>`.
- `<change-worktree>` is where the work on the change happens. Its standard location is `<repo>/.worktrees/<change-id>`. If the harness or IDE supplied a worktree, use it, and symlink it to `<repo>/.worktrees/<change-id>` for consistency and easy lookup.

`.changes` and `.worktrees` should both be gitignored. If they aren't, tell the user.

### Intent commits

Intent commits are commits that include changes to `_intent/`. They contain only intent: commit code and other changes separately.

- Omit co-authorship trailers such as `Co-Authored-By`, even if your harness adds them by default, unless you directly authored the intent.
- Quote the intent in the commit message where possible to convey its meaning. Light editing and summarizing are fine.
- If the user gives a reason or rationale when asking for the commit, include it. If they don't, you may ask for one, but don't insist.

## P1. Establish change identity

Determine the nature of the change, from the initial prompt or from talking with the user.

When ready:

1. Pick the `<change-slug>` and compute the `<change-id>` (`date +%Y-%m-%d-%H%M`).
2. Create the `<change-dir>`.
3. Create the `<change-worktree>` on `<change-branch>`, branching from the repo's default branch unless the user says otherwise:
   `git -C <repo> worktree add -b change/<change-slug> <repo>/.worktrees/<change-id> <base>`

Tell the user the full paths to the change directory and worktree so they can easily open them in an IDE.

## P2. Requirements and design

Work from the `<change-worktree>`.

Watch `<change-worktree>/_intent` for changes and react to them, with a debounce of about 30 seconds to avoid flapping. If the harness can stream a background command's output (in Claude Code, the Monitor tool), run the bundled watcher:

```bash
bash <this-skill-dir>/scripts/watch.sh <change-worktree>/_intent 30
```

It prints one line naming the changed files each time edits settle. Read the diff (`git -C <change-worktree> diff -- _intent/`) and respond.

If the harness can't watch, tell the user, and look for changes whenever they respond. Remind them they can say "c", "continue" or something else short for you to loop.

Encourage the user to start by thinking about requirements. They may or may not; go with what they choose. Requirements are optional in intent files, but they are a very good place to start, and sometimes to end.

The goal is to specify the change so an implementing agent won't find it confusing or conflicting. Do not encourage excessive detail: vague intent is fine if an implementing agent can likely build it reasonably.

Help the user:

1. Answer questions.
2. Suggest changes.
3. Check for mistakes and inconsistencies: undefined or inconsistently used terms, contradictions, broken links, etc.

When the changes are done and ready for implementation, you may help the user:

1. Commit the changes, following the intent commit rules above.
2. Push them and create the PR. Quote the user's intent as much as possible in the description.

The user may get outside review before implementing; that's up to them.

## Handing off to implementation

Offer to implement the changes when they're ready.

If useful, create `<change-dir>/notes.ai.md` for the implementing agent. Notes do not take the place of intent. They are supplemental context from your conversation with the user, steering the implementation toward what the user most likely wants without deviating from the intent.

Use a sub-agent with the `hdd-implement` skill to implement the changes. Give it the `<change-worktree>`, the `<change-dir>` and the base branch. If the `hdd-implement` skill isn't available, tell the user.

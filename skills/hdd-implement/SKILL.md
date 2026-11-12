---
name: hdd-implement
description: Implement a human-driven-development (HDD) change from a diff of the project's `_intent/` directory, in rounds — plan, implement with sub-agents, log decisions/assumptions/conflicts/defects, run the repo's checks and CI, and write a round report that leads with suggested intent changes. Use when the user asks to implement intent changes, says "hdd implement", or when hdd-change hands off implementation.
---

# HDD implement

Implement changes to the software based on a diff of intent. Use it any time there is an intent diff to implement: iteratively for small changes, or as a long-running task for larger ones.

Be flexible on process but firm on human authorship of intent: never edit `_intent/`. When intent should change, suggest the change to the user.

## Conventions

These are shared by all HDD skills.

- `<repo>` is the root of the primary checkout of the repo.
- `<change-slug>` is a short kebab-case identifier for the change, e.g. `add-profile-page`, `fix-loading-animation`.
- `<change-id>` is the current local timestamp plus the slug: `YYYY-MM-DD-hhmm-<change-slug>`, e.g. `2026-11-07-0030-intent-skills`.
- `<change-dir>` is `<repo>/.changes/<change-id>`. It holds ephemeral content for the change, is gitignored and is managed by agents.
- `<change-branch>` is `change/<change-slug>`.
- `<change-worktree>` is where the work on the change happens. Its standard location is `<repo>/.worktrees/<change-id>`. If the harness or IDE supplied a worktree, use it, and symlink it to `<repo>/.worktrees/<change-id>`.

### Commits

Intent commits include changes to `_intent/` and contain only intent. Omit co-authorship trailers such as `Co-Authored-By` from them unless you directly authored the intent. Quote the intent in the message where possible; light editing and summarizing are fine.

Code commits never include `_intent/` changes.

## Setup

Determine what intent diff you're being asked to implement. Common cases:

1. **A PR exists.** Its base branch is the base. Everything should be committed and pushed.
2. **A bare feature branch.** Offer to create the PR. Assume `main` as the base unless there's a reason to use something else (like `dev` on some repos).
3. **Uncommitted intent changes.** Offer to commit them (as an intent commit), create the `<change-branch>` and the PR.
4. **A backfill checkpoint.** `hdd-backfill` hands you `.gen.md` files describing the current code. See [Backfill rounds](#backfill-rounds).

If there's no PR, use your judgement for the base: `main`, `dev`, `trunk`, or whatever seems correct for the repo. Stop and ask the user if it isn't clear.

Ensure the `<change-slug>` has been chosen; choose it if not. Create the `<change-dir>` and `<change-worktree>` if they don't exist yet.

## Rounds

Implementation happens in rounds: the agent implements, the user reviews and updates intent, the agent implements again.

Each round has a `<round-id>` of the form `round-##`, starting at `round-01` and incrementing. Its `<round-dir>` is `<change-dir>/<round-id>`.

### Starting a round

1. Commit any uncommitted intent changes as an intent commit.
2. Record the commit the round starts from: `git rev-parse HEAD > <round-dir>/start-commit`.
3. Compute the round's intent diff:
   - Round 1: from the merge base with the base branch to this round's start commit.
     `git diff $(git merge-base <base> <start>) <start> -- _intent/`
   - Later rounds: from the previous round's start commit to this round's start commit, so the round sees only intent added since the previous round began.
     `git diff <previous-start> <start> -- _intent/`
   - If a recorded commit no longer exists (e.g. after a rebase), stop and ask the user which base to use.
4. Read `<change-dir>/notes.ai.md` if it exists. It is supplemental context; it never overrides intent.

### Backfill rounds

A backfill checkpoint is a round whose intent diff comes from `.gen.md` files rather than git history. For each `<name>.gen.md` beside an intent file, the diff is from the `.gen.md` to the user's intent:

`git diff --no-index -- <name>.gen.md <name>.md`

The user has already reconciled the differences, so the remaining ones are changes they chose to keep; the caller may pass that list. Implement those, and expect little else to change. The round still records its start commit as usual, so later rounds diff from the checkpoint.

### Implementing

Interpret the diff as described in [Interpreting intent](#interpreting-intent). Plan the changes, writing planning artifacts in the `<round-dir>`. Use sub-agents, concurrency and best practices for implementing.

The root implementing agent keeps a log in `<round-dir>/impl-log.ai.md`. Sub-agents report their decisions, assumptions, conflicts and defects back to the root agent, which logs them.

Each log record has a type, an anchor and a note. The anchor is the most relevant file or command. One record per line:

```md
- **Decision** · `src/devices/list.rs` · Grouped USB and host-serial links together; intent says "USB" without distinguishing them.
```

Record types:

1. **Decision:** you made an implementation decision where intent left something unclear.
2. **Assumption:** you assumed something that, if untrue, might make the implementation wrong.
3. **Conflict:** you found intent in conflict with itself, the code, best practices or another concern (see [Intent conflicts](#intent-conflicts)).
4. **Defect:** you accepted a minor defect as won't-fix, such as an unlikely race condition or validation issue.

### Ending a round

Run the standard correctness checks:

1. Run formatting, linting, build, tests and anything else the repo calls for. Look at the repo's scripts, task runner and CI config to find them.
2. Commit the code and push the `<change-branch>` so CI runs. Wait for CI and fix any problems.
3. If automated code review is configured in CI, fix any high-priority problems. Fix minor issues at your discretion; log any you leave as a **Defect**.

Then write the round report at `<change-dir>/<round-id>-report.ai.md`. It sits beside the `<round-dir>`, not in it, so the user can find it easily. In order:

1. **Suggested intent changes**, for clarification or correction, as diffs with markdown links to the intent files. Omit the section if there are none.
2. **Summary** of the implementation, based on the log.
3. A link to the log.

Tell the user the report's full path. If you were started by another agent (e.g. from `hdd-change`), return the path to it.

## Interpreting intent

Intent is not always specific, by design. When interpreting it, balance:

1. **Minimize change:** make the smallest reasonable change that matches the intent.
2. **Maximize correctness:** follow the intent and keep the software consistent.
3. **Best practices:** follow best practices for the problem and domain; log your assumptions.

For brownfield or existing code, err on the side of minimizing change. For greenfield or new code, prioritize correctness and best practices.

When intent changes but the software could satisfy both the old and new versions, be pragmatic:

- If the user appears to be just rewording or reorganizing intent, make no code changes, and log a **Decision** saying so.
- If the user seems to be refining a concept or specifying something new, match it.

## Intent conflicts

While implementing, you may find that intent conflicts with itself, the code, best practices or any other concern.

- **Minor** (unlikely to cause major issues, at your discretion): continue, and log it as a **Conflict**.
- **Major:** stop the round and notify the user of the problem with a suggested remedy. The remedy is often a suggested intent diff with a link to the file. A new round starts after the user corrects the problem.

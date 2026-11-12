# HDD implement skill

This skill implements changes to the software based on a diff of intent.

It can be applied any time there is an intent to diff, iteratively for small changes or as a long-running task for larger ones.

Before starting, determine what intent diff you're being asked to implement.
Common cases:

1. PR base branch. In this case, everything should be committed and pushed.
2. Bare feature branch. In this case, offer to create the PR. Assume `main` as the base unless there is a reason to use something else (like `dev` on some repos).
3. Uncommitted changes. In this case, offer to commit changes, create the branch, and the PR.
4. A `.gen.md` intent file used for backfill.

Be flexible on process but firm on human-authorship of intent.

Ensure the `<change-slug>` has been chosen, choose it if not.

Create the `<change-dir>` and `<change-worktree>` if not already done.

## Implementation rounds

In HDD, implementation happens in rounds:

- the agent implements
- the user reviews and updates intent
- the agent implements again

Each round gets a `<round-id>` in the form `round-##` such as `round-01`.
The rounds start at `01` and increment for each round.

Rounds record the git commit hash they started from.
Before starting, commit any uncommitted intent changes.

Rounds use the intent diff from the previous round.
The first round uses the PR base branch.
When there is no PR, the agent uses its judgement: `main`, `dev`, `trunk`, or whatever seems correct for the repo.
Stop and ask the user if it isn't clear which branch to use.

Plan your changes, writing planning artifacts in the `<round-dir>`: `<change-dir>/<round-id>`.
Use sub-agents, concurrency, and best practices for implementing changes.

The root implementing agent keeps a log in `<change-dir>/<round-id>/impl-log.ai.md`.

The log consists of records, each with a type, anchor, and note.
The anchor is the file or command most relevant.
Sub-agents report log entries to the root agent for adding to the log.

The types are:

1. **Decision**: the agent made an implementation decision where intent left something unclear.
2. **Assumption**: the agent made an assumption about something that, if untrue, might cause the implementation to be wrong.
3. **Conflict**: the agent found a conflict in the intent.
4. **Defect**: the agent accepted a minor defect as won't fix, such as an unlikely race condition or validation issue.

Rounds end with standard correctness checks:

1. Run formatting, linting, build, tests, and anything else the repo calls for.
2. Push changes to CI and let it run; fix and problems.
3. If automated code review is configured in CI, fix any high-priority problems; minor issues may be fixed at agent's discretion, and should be included in the log as a defect.

At the end of implementation, the root agent generates `<change-dir>/<round-id>-report.ai.md`.
The report is kept out of the `<round-dir>` for discoverability by the user.

The report contains a summary of the implementation, based on the log, with a link to the log.
Near the top, it contains any suggested intent changes, for clarification or correction.
These changes are suggested in diff format, with markdown links to the files.

## Interpreting intent

Intent is not always specific, by design.

When interpreting intent, balance these concerns:

1. Minimize change: make the smallest reasonable change that matches intent.
2. Maximize correctness: try to follow the intent and keep the software consistent.
3. Best practices: follow best practices for the problem and domain; log your assumptions.

For brownfield/existing code, err on the side of minimizing change.
For greenfield/new code, prioritize correctness and best practices.

When the intent changes but the software could satisfy both new and old, be pragmatic.
If it appears the user is just rewording or reorganizing intent, make no changes; log this as a decision.
If the user seems to be refining a concept or specifying something new, try to match it.

## Intent conflicts

While implementing, the agent may find that the intent is in conflict with itself, the code, best practices, or any other concern.

If the conflict is not likely to cause major issues (agent's discretion) it should continue and log the conflict.

If the conflict is major, the round is stopped, and the user notified of the problem with a suggested remedy.
The remedy is often in the form of a suggested intent diff with a link to the file.

A new round starts after the user corrects the problem.

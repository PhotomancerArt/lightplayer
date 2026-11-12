# HDD skills

The skills defined in this directory implement the human-driven-development process.

Concepts shared across the skills are defined here.

Skills are defined at `<repo>/skills` and are symlinked for use with Claude and Codex in the project (e.g. `<repo>/.claude/skills`)

## Repo and worktree

`<repo>` refers to the root of the primary checkout of the repo.
`<change-worktree>` refers to the worktree for the change.

## Change slug and id

The `<change-slug>` is a short `kebab-cased` identifier for a change.
Examples: `add-profile-page`, `rework-navbar`, `fix-loading-animation`, `redact-secrets`

The `<change-id>` is the current local timestamp plus the slug: `YYYY-MM-DD-hhmm-<change-slug>`
Example: `2026-11-07-0030-intent-skills`

## Change directory

The `<change-dir>` is a directory that holds ephemeral content related to that change.

The standard location is: `<repo>/.changes/<change-id>`

It is gitignored.

It is managed by agents.

## Change branch

The `<change-branch>` is the git branch where the change is made.

The standard name is `change/<change-slug>`

## Change worktrees

Worktrees are where the work on a change happens.

The standard location is: `<repo>/.worktrees/<change-id>`

Agent harnesses and IDEs sometimes supply the worktrees.
If provided, use it, but symlink it to `<repo>/.worktrees/<change-id>` for consistency and easy lookup.

## Intent commits

Intent commits include changes to the `_intent/` directory.

They should contain only intent.

They should omit any co-authorship indicator if possible, such as "Co-Authored-By", unless the agent directly authored the intent.

Their commit messages should quote the intent if possible to convey meaning.
Light editing and summarizing are acceptable.

Rationale: humans often commit with `git add -A` and very simple commit messages.
Agent-commits tend to be cleaner and more targeted.

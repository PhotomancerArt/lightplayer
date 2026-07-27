---
status: fixed
found: 2026-07-27      # how: report
fixed: this change
area: dev tooling (.claude/launch.json + scripts/dev-port.sh)
class: assumed-context
related:
  - docs/adr/2026-07-27-worktree-local-launch-json.md
  - docs/process/review-gates.md
---
# Tracked launch.json pinned one worktree's dev port on every worktree

**Symptom** — During a visual gate, Yona was given a dev-server link that
served a *different worktree's build*; the review answer applied to the wrong
work and the gate had to be re-run. Separately, agents kept overriding
`STUDIO_WEB_PORT`, causing cross-worktree port confusion.

**Root cause** — Dev ports are per-worktree by design (`scripts/dev-port.sh`
hashes worktree root + service), but `.claude/launch.json` — the file the
harness browser pane reads for its port — was **tracked in git** with
`"port": 27395, "autoPort": false`. 27395 is the hash port of the spike
worktree that committed the file (92884ba85); no current checkout hashes to
it (main: 29766, worktrees: various). Every session's launch config asserted
a port that its own context could never produce. Agents then either attached
the pane to whatever stale server sat on the pinned port, or pinned
`STUDIO_WEB_PORT` to force their server onto it — both re-creating exactly
the collisions the hash design prevents.

**Fix** — `.claude/launch.json` is now gitignored and generated per worktree
by `just claude-launch-json`, using a new side-effect-free
`scripts/dev-port.sh --query` mode so the declared port always equals the
port `just studio-dev` will pick. Process rules added: AGENTS.md hardens
"never pin a port without the user asking in chat";
`docs/process/review-gates.md` + the `lp-review-handoff` skill make the
handoff verify pane-shows-this-worktree before sharing a link.

**Regression coverage** — none: config + process defect; there is no test
seam for the harness's launch-file consumption. The structural guard is the
gitignore (the file can no longer be committed) and the generator being the
only documented way to produce it.

**Lesson** — A committed config file asserting a value that is *computed
per environment* is a standing lie: it is correct in at most one checkout
and silently wrong everywhere else, and readers (human or agent) trust it
precisely because it is checked in. When a value is derived from context
(worktree, machine, session), the artifact carrying it must be generated in
that context — tracked files may carry the generator, never the value.

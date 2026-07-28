# ADR: `.claude/launch.json` is generated per worktree, never tracked

- **Status:** Accepted
- **Date:** 2026-07-27
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Dev-server ports in this repo are per-worktree by design:
`scripts/dev-port.sh` hashes (worktree root, service) into 20000–39999 so
parallel agent worktrees coexist without stealing each other's servers
(see AGENTS.md "Dev server ports").

The Claude harness reads `.claude/launch.json` to open its browser pane at a
declared port. That file was tracked, carrying `"port": 27395` — the hash
port of the long-gone worktree that committed it. Every other worktree's
`just studio-dev` lands elsewhere (this worktree: 20294; the main checkout:
29766), so the harness pane pointed at a port that matched no correctly
started server. Agents were pushed into two failure modes: attaching to a
stale server from another session, or pinning `STUDIO_WEB_PORT` to force
their server onto the committed port. On 2026-07-27 this caused Yona to
review the wrong worktree's build and give an incorrect gate answer
(`docs/defects/2026-07-27-launch-json-pinned-port.md`).

A static tracked file cannot express a per-worktree computed value.

## Decision

`.claude/launch.json` is a per-worktree generated artifact:

- It is gitignored (`/.claude/launch.json`) and was removed from tracking.
- `just claude-launch-json` writes it, deriving the port from
  `scripts/dev-port.sh --query studio-dev` — the same hash `just studio-dev`
  will use. `--query` is side-effect-free (no eviction, no probing).
- Agents regenerate it (idempotent) before opening a harness preview, and
  never hand-edit a fixed port into it or pass `*_PORT` pins without the
  user asking.

The recipe currently emits only the `studio-dev` entry; ad-hoc local entries
can be added by hand precisely because the file is untracked.

## Consequences

- The harness pane and the actual server agree in every worktree.
- Checkouts that still carry the old tracked file lose it on merge; running
  `just claude-launch-json` once restores a correct local copy.
- If the hash slot is occupied by a foreign worktree at launch time,
  `studio-dev` probes upward while the generated file still names the hash
  slot — the recipe's printed URL remains the source of truth, and the rare
  mismatch is visible rather than silent. Regenerate + restart resolves it.
- Other services (e.g. `fw-browser-smoke`) can be added to the generator if
  harness previews ever need them.

## Alternatives Considered

- **Keep a tracked file and have agents edit the port per worktree** —
  dirties a tracked file in every worktree and invites committing a new
  wrong pin; rejected.
- **Pin `STUDIO_WEB_PORT` to the committed port** — reintroduces cross-
  worktree port fights that the hash design exists to prevent; rejected.
- **Make `studio-dev` honor launch.json's port** — inverts authority: the
  launch file would become the port oracle, and two worktrees sharing the
  committed file would collide; rejected.

## Follow-ups

- None.

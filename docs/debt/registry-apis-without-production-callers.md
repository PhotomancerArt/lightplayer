---
status: carried
since: 2026-07-04
logged: 2026-08-01
area: lpc-registry / lpa-server project APIs
related:
  - "../adr/2026-07-04-studio-editing-model.md"
  - "../adr/2026-08-01-debug-slots-taxonomy.md"
  - "plan notes: ~/.photomancer/planning/lp2025/2026-07-31-1736-ephemeral-slots/notes.md (S7)"
---
# `ProjectRegistry::discard_overlay` is public API with no production caller

**Shape** — `ProjectRegistry::discard_overlay`
(`lp-core/lpc-registry/src/registry/project_registry.rs:444`) clears the
whole overlay and re-derives the inventory. Nothing in production calls
it: the studio's "Revert to saved" goes through `MutationOp::Clear` on
the ordinary mutation path, and the only caller is
`lp-core/lpc-registry/tests/apply.rs:188`. So the method is exercised
exclusively by the test that exists to exercise it — a shape that reads
as coverage but proves nothing about a path anyone takes.

This is a condition rather than a one-off deletion because the registry
has more than one of these: a public surface accretes recovery-shaped
entry points ("discard everything", "reload from disk") that seem
obviously useful, are cheap to keep compiling, and quietly diverge from
the paths that actually run. Each one is a second implementation of a
behaviour the real path already has, and the divergence surfaces only
when someone finally wires it up. `Project::reload()` is the sibling
case (see `project-reload-drops-debug-silently.md`).

The Debug-slots work is a concrete example of the divergence risk: the
mutation path grew role-aware retention and validation, and a bypassing
"clear it all" entry point is exactly the kind of thing that would not
have been updated with it.

**Carrying cost** — Low but real: dead API widens the crate's contract,
its test consumes gate time while asserting nothing about production,
and every refactor of overlay semantics has to reason about a caller
that does not exist. It cost one triage pass during the Debug-slots plan
(catalogued as S7).

**Workarounds** — When changing overlay semantics, treat `discard_overlay`
as a mirror of the `MutationOp::Clear` path and update both, or the next
person to wire it up inherits a stale behaviour.

**Incident log**
- 2026-08-01 — re-confirmed during the Debug-slots plan (P6 paydown
  sweep): still no production caller. Logged rather than deleted; the
  paydown list for that plan was closed at three fixes, and deleting a
  public API deserves its own look at whether the recovery story wants
  it.

**Exit criteria** — Either `discard_overlay` gains a real caller (a
recovery flow that needs a non-mutation clear) with a test that goes
through that caller, or it is deleted along with its test and
"revert everything" stays the single `MutationOp::Clear` path.

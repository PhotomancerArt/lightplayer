---
status: carried
since: 2026-07-04
logged: 2026-08-01
area: lpa-server project lifecycle
related:
  - "../adr/2026-08-01-debug-slots-taxonomy.md"
  - "../adr/2026-07-04-studio-editing-model.md"
  - "registry-apis-without-production-callers.md"
  - "plan notes: ~/.photomancer/planning/lp2025/2026-07-31-1736-ephemeral-slots/notes.md (S8)"
---
# `Project::reload()` discards the whole overlay with no signal

**Shape** — `Project::reload()`
(`lp-app/lpa-server/src/project.rs:397`) is documented as a recovery
path: "discard live runtime state and rebuild from the committed
filesystem". It drops the runtime and rebuilds registry and engine from
`ProjectLoader::load_from_root`, which means the device-side overlay
goes with it — every pending edit, and every **Debug** override
(`clock.controls.*`, `output.test_pattern`). Persisted edits are
recoverable in principle (the client mirror can re-stage them); Debug
overrides are not authored anywhere, so they are simply gone.

Dying on reload is the *correct* lifetime for a Debug value — the
taxonomy ADR makes it a deliberate property ("a rebooted installation
must not come up in test-pattern mode"), and overlay persistence across
restarts was explicitly rejected ("a device-crashing edit must not
crash-loop"). The debt is that the drop is **silent and unaccounted**:
no return value, no event, no notice mentions that pending state
vanished, and the client would discover it only through the next
overlay read. Today that costs nothing, because `reload()` has no
production caller (it is the documented future recovery path). It
becomes a real defect the moment recovery is wired up — the flow that
calls it is exactly the flow where a user has a reason to be confused
about what they just lost.

**Carrying cost** — Zero today, deferred and concentrated: the eventual
recovery flow has to (re)discover the semantics and design the
messaging, and the current signature gives it nothing to report.
Catalogued as S8 during the Debug-slots discovery sweep.

**Workarounds** — Do not call `reload()` from a user-facing flow without
first deciding what the user is told. The information needed for a good
notice is available before the drop: `registry.overlay()` can be
counted (and classified — `SlotRoleResolution::persistence`) at the top
of the function.

**Incident log**
- 2026-08-01 — re-verified during the Debug-slots plan (P6): still no
  production caller, and the D7 change makes the loss *more* invisible
  than before (Debug edits no longer appear in the Save panel, so
  nothing on screen hints they existed). Logged, not fixed.

**Exit criteria** — The first production caller lands together with a
decision about the drop: either `reload()` reports what it discarded
(counts, split persisted vs Debug) so the flow can say so, or the
recovery flow re-stages the client mirror's persisted edits and tells
the user Debug overrides were cleared.

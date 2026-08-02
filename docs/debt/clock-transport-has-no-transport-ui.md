---
status: carried
since: 2026-05-12
logged: 2026-08-01
area: clock node / studio node faces
related:
  - "../adr/2026-08-01-debug-slots-taxonomy.md"
  - "../adr/2026-07-26-node-card-faces.md"
  - "plan notes: ~/.photomancer/planning/lp2025/2026-07-31-1736-ephemeral-slots/notes.md (S9, D6)"
---
# The clock's transport controls have no transport UI

**Shape** — `ClockControls`
(`lp-core/lpc-model/src/nodes/clock/clock_controls.rs`) exposes
`running`, `rate`, and `scrub_offset_seconds` as Debug-role slots, and
the engine consumes all three every frame. The UI for them is the
generic slot renderer: a toggle, a number, and — for the scrub offset —
a plain slider whose unit is "seconds added to the clock". There is no
transport surface: no timeline, no scrub bar, no jog, no
return-to-zero, no readout of where the show currently is. Scrubbing a
show by typing an offset into a numeric field is the tell.

It is structural rather than a missing widget because of *where the
controls live*. Debug is defined as diagnostics/authoring overrides with
no durable value underneath, and `running`/`rate`/`scrub_offset_seconds`
only half fit: driving time by hand to inspect a show is diagnostics,
but transport is a first-class performance concept that wants its own
home (project-level, not buried in one node's card). The taxonomy ADR
records this as the known tension in the `Debug` name, and expects it to
resolve by those controls moving to a transport surface — at which point
Debug holds exactly the diagnostics it describes. Building a scrub UI
inside the clock's Debug section now would cement the wrong home.

**Carrying cost** — Inspecting a show at a chosen time is clumsy enough
that it mostly is not done; the scrub slot reads as unfinished; and the
`Debug` category carries an example that undercuts its own definition,
which costs explanation every time the taxonomy is taught.

**Workarounds** — Set `controls.scrub_offset_seconds` in the clock
card's Debug section and read the resulting time from the clock's
produced state; Clear (per value or per node) returns to live time.

**Incident log**
- 2026-08-01 — catalogued as S9 in the Debug-slots discovery sweep and
  cited in D6 as the reason `Debug` is ratified as provisional. The
  Debug section (P3) at least makes the controls findable and marks the
  system as debug-driven while an offset is held; the transport gap is
  untouched.

**Exit criteria** — A transport surface exists (scrub/rate/run at the
project level, with a position readout), the clock's three controls move
onto it, and the `Debug` naming re-check in
`../adr/2026-08-01-debug-slots-taxonomy.md` (follow-up (a)) can be
answered against a Debug category that holds only diagnostics.

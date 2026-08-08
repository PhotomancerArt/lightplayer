---
status: resolved
since: 2026-05-12
logged: 2026-08-01
resolved: 2026-08-05
area: clock node / studio node faces
related:
  - "../adr/2026-08-01-debug-slots-taxonomy.md"
  - "../adr/2026-07-26-node-card-faces.md"
  - "../adr/2026-08-07-clock-transport-is-a-panel-instrument.md"
  - "plan notes: ~/.photomancer/planning/lp2025/2026-07-31-1736-ephemeral-slots/notes.md (S9, D6)"
  - "plan: ~/.photomancer/planning/lp2025/2026-08-04-2355-clock-tape-hero/"
---
# The clock's transport controls have no transport UI

**Shape** — `ClockTransport`
(`lp-core/lpc-model/src/nodes/clock/clock_transport.rs`, named
`ClockControls` at `controls` until plan
`2026-08-04-2355-clock-tape-hero` P1) exposes
`running`, `rate`, and `scrub_offset_seconds` as Debug-role slots (P6 of
the same plan retyped `running` to `play_state: PlayState`), and
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

**Workarounds** — Set `transport.scrub_offset_seconds` in the clock
card's Debug section and read the resulting time from the clock's
produced state; Clear (per value or per node) returns to live time.

**Incident log**
- 2026-08-01 — catalogued as S9 in the Debug-slots discovery sweep and
  cited in D6 as the reason `Debug` is ratified as provisional. The
  Debug section (P3) at least makes the controls findable and marks the
  system as debug-driven while an offset is held; the transport gap is
  untouched.
- 2026-08-04 — **the engine half now exists.** The TimeProduct work
  (plan `2026-08-04-0003-timeproduct-m2-core`, P8) added a breakpoint
  log to the timebase store: per-phasor `Breakpoint { t_eff, phase,
  cycle, rate }` segments behind the default-on, host-only `scrub-log`
  feature on `lpc-engine`, trimmed to a 30 s window. Reads at a
  scrubbed-back effective time are answered by closed-form segment
  lookup and are **bit-exact** against the values the live path
  produced, so a future scrub bar can move time freely without the
  motion changing. Devices keep the forward-only integrator with no
  log (nm/strings on the rv32 ELF find no log symbols), and follow a
  backward scrub through the clock's now-signed `delta_seconds`.
  Nothing user-facing changed: the only way to drive it is still the
  generic Debug slider, so the exit criteria below are untouched — the
  half that was hard is simply no longer the blocker.
- 2026-08-05 — **RESOLVED by the tape transport** (plan
  `2026-08-04-2355-clock-tape-hero`, P3–P5 on PR #345, after the spike's
  two gate rounds chose the tape direction). The clock card's OUTPUT
  hero is now a transport instrument: a scrolling tape strip under a
  fixed playhead (drag = scrub, px = seconds), a log ׼–×8 speed fader
  with magnetic octave detents, run/pause, calm m:ss digits, and an
  amber off-live chip with tap-to-return. The three slots stay
  `SlotRole::Debug` (transient by design — Q2) but their generic rows
  no longer render anywhere: the face claims them
  (`retire_face_claimed_debug_rows`, `node_controller.rs`), and the
  tape carries the debug affordance itself — changed controls tint
  attention-orange and one `clear` affordance drops every override.
  The surface is card-level (and P8 puts the same instrument on the
  module panel, as one grouped Transport control over the three
  `clock.*` channels P6 materialized),
  not the project level the exit criteria guessed — the clock IS the
  project's timebase, so its card is the transport's natural home.

**Exit criteria** — MET 2026-08-05, with one deliberate refinement: the
transport surface lives on the clock card (+ module panel, P8) rather
than a separate project-level home. The `Debug` naming re-check
(taxonomy follow-up (a)) is answered by
`docs/adr/2026-08-07-clock-transport-is-a-panel-instrument.md` §6: with
the transport rows retired into a real instrument, the drawer's
remaining in-tree example is pure diagnostics (`OutputDef::test_pattern`).

**Closed 2026-08-07** — the module-panel half (P6/P8) landed: one
grouped Transport control on the panel, wired per-leaf onto the three
`clock.*` bus channels, exposed by default (`panel = "show"` on the
`transport` record). Every exit criterion above is now satisfied on both
surfaces named in the refinement. See
`docs/adr/2026-08-07-clock-transport-is-a-panel-instrument.md` for the
full architecture record, including the option-B correctness argument
that shaped the panel half.

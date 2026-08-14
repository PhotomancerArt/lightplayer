# ADR: Patch-selection pulse — the `highlight` Debug slot

- **Status:** Accepted
- **Date:** 2026-08-10
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-08-01-debug-slots-taxonomy.md` (the slot's category and
  lifecycle; this ADR adds the second in-tree Debug slot);
  `2026-08-10-output-fragments-and-patch-files.md` (the placements the
  subject mapping reads); the mapping & patching slice-2 plan's D43/Q27
  (planning dir `2026-08-09-2332-mapping-patching-slice2`, where the pulse
  was cut from the slice and chipped).

## Context

Patching is done standing at a rig: the question the surface cannot answer
from a browser tab is *which physical strand is `/sector/2`*. lp2014 had
`debugChannels` for exactly this, and ~90% of its clicks were dome
patching. Slice 2 shipped the patch surface with a client-side highlight
only (D43's free half); the sim/hardware pulse was cut to keep the
unattended run tight (Q27, ruled at planning) and built here at the
controller/engine seam — deliberately not bound to the interim `/patch`
page, because the patching UI is being re-housed inside the mapping
editor and the seam must outlive the move.

## Decision

**One mechanism serves sim and hardware: a second Debug slot on
`OutputDef`, `highlight: ValueSlot<String>`, painted by the output node
over its live frame.**

- **The value is a lamp-span microformat** — inclusive ranges in the
  output's flat wire numbering, `"0-29,45,90-119"`, the same numbering
  patch entries anchor with `at.lamp`. A string, not a structured value,
  because the Debug section renders string fields editable today: the
  slot is hand-drivable from the output card with zero new UI, which is
  also the manual test path. Unparseable segments are skipped — a
  diagnostic must never stop an output pushing pixels.
- **The engine paints an OVERLAY, not a bypass.** Unlike `test_pattern`,
  the graph keeps rendering; after the real render the named lamps are
  repainted with a 2 Hz blink between the test-pattern white and dark.
  White guarantees contrast on dark content, dark on bright content —
  the selection is findable on any background, in the context of the
  running show. With the slot empty the render path publishes exactly
  the bytes it always did (the A1 byte-identity discipline).
- **The controller seam is `PatchPulseOp { subject }`** (dispatched to
  `ProjectController`, like the other project ops). Subjects speak the
  two spaces a patch maps between (D32v): `Fixture { node, range }` in a
  producer's own lamp numbering — object instances and range selections
  both reduce to this — and `Output { node, range }` in wire space,
  where ports already live (Q21: ports are UI grain; a port selection IS
  the span its port table renders). The controller maps fixture-space
  subjects through the published `WireOutputPlacement`s (authoritative,
  resolver-order, the same data the patch bay draws) and writes each
  involved output's `highlight`; outputs the previous pulse touched and
  this one does not are swept with Clear. `subject: None` clears
  everything.
- **The writes are ordinary Debug-slot overlay edits** — no wire change,
  no dirty weight, no Save-panel presence, and they reach whatever runs
  the engine: the browser sim and a connected board take the identical
  path (`lpa-server` is embedded in both). Per the taxonomy, the value
  survives a client disconnect (the overlay lives device-side) and dies
  on unload/reboot — a rebooted installation never comes up pulsing.

## Consequences

- Selecting a patch subject can light the physical lamps it names, on
  hardware, with no new wire surface and no new UI machinery — the
  Debug section treatment, Clear verbs, and header chip all arrived
  free by marking the field.
- `DebugSlotsSection` now has two occupants (`test_pattern`,
  `highlight`), both pure diagnostics — the good outcome the taxonomy
  predicted, and an amendment to its "one occupant" follow-up note.
- The pulse holds while set: a user who selects a subject and walks to
  the rig sees it pulsing when they get there, at the cost that an
  abandoned session leaves it pulsing until cleared or unloaded. That
  is `test_pattern`'s exact trade, accepted deliberately (no TTL —
  the taxonomy already rejected leases for state with a durable home).
- A patched fixture's pulse is honest per-run: a range straddling a
  reversed run pulses the lamps where they actually sit (the tail
  re-base), which is precisely the "which strand IS this" answer.

## Alternatives Considered

- **A runtime command (`WireNodeCommand`) with a TTL.** Rejected: the
  command channel is for events; the pulse is ephemeral *state* the
  engine must see every frame — the Debug slot is its ratified home,
  and PR #233's lease machinery was already rejected for this shape.
- **Client-side only (no engine surface).** Ships in the surface
  already (D43's free half) and does not answer the question — the
  physical rig is where the strand is.
- **A structured list slot (`U32ListSlot`-style spans).** Works over
  the overlay, but composite values render read-only in the Debug
  section today; the string microformat keeps the slot hand-drivable
  and legible. Revisit if the microformat ever needs richer content
  (per-span colors, say).
- **Binding the op to the `/patch` page's selection directly.** The
  page is interim (G1: patching re-houses inside the mapping editor);
  the op takes subjects, not page state, so the re-housed UI calls the
  same seam.

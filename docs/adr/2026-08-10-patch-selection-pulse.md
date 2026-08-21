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

## Amendment 2026-08-20 — CHASE joins the breath (microformat v2)

The slot now carries **two light languages**, because a patch selection
asks two different questions. "Which strand IS this?" (wire-side) is
answered by the breath this ADR decided. "Does this object run the way I
think it does?" (fixture-side) needs DIRECTION, which a symmetric white
fade cannot say. Design record: `spikes/patching-controls/index.html` §3
(the ruling matrix and its `chaseRgb`) and the walk-up assignment plan
(`2026-08-20-1826-patching-pass2-walkup-assignment`, D1/D9/D10).

**Microformat v2** — the slot stays `ValueSlot<String>`, still a Debug
slot, still hand-drivable from the Debug section:

```text
value   := [ "chase:" ] list         ; prefix is case-insensitive
list    := segment { "," segment }
segment := lamp | lamp "-" lamp      ; inclusive on both ends
```

- **No prefix** is v1, unchanged in meaning and in bytes: unordered wire
  spans, breathing white, inverted ranges skipped. Nothing that ever
  wrote this slot has to change.
- **`chase:`** lists the spans in **object order** — the first segment
  holds object lamp 0 — and a **descending** range (`59-0`) means that
  run is walked backward on the wire. `chase:60-119,59-0` is one object
  whose second half is plugged in at the far end.
- Junk segments are still skipped; an **unknown prefix paints nothing**
  rather than guessing a language. An empty or unparseable value is the
  byte-identity no-op the original decision pinned.

**The chase's look** (D10, ratified in the spike): the first and last
`clamp(1, 10, round(n / 10))` lamps of the object wear blue `#0000ff`
and red `#ff0000`; the body sits near-dark (25/255) with one full-white
dot sweeping head-to-tail once every 2 s, on a raised window that fades
out a seventh of the object either side. `n` is the object's total lamp
count across all its spans, so the ends stay legible on a 12-lamp arch
and on a 3000-lamp dome. Everything outside the named spans dims `>>2`,
exactly as the breath does — the two languages share their first move.

**Selection kind → language** (the spike's §3 matrix; the engine only
honors what the string says, the client chooses):

| selection | language |
|---|---|
| fixture object / instance / range / cell / mapped segment | CHASE |
| output / port / free segment | BREATH |
| nothing | show content |

**A2 — layout fallback.** Breath paints white, which is
channel-order-agnostic; blue and red are not. The chase resolves every
named lamp's channel order from the output's own published sample
layout (`ControlSampleLayout`, the `RgbPixels { color_order }` runs the
producers declare and already render in), and packs its colors in that
order. **If any named, in-extent lamp does not resolve to an RGB run —
no layout published yet, a `Raw` run, or a gap — the whole output falls
back to the breath for that frame and paints no chase at all.** Refusing
per-output rather than per-lamp is deliberate: a chase that decodes half
an object names the wrong end of the strand, and a wrong direction claim
is worse than an honest direction-free one. The same fallback catches
absurd input: a chase naming more than 65 536 lamps costs a breath, not
a four-billion-lamp frame (the chase is per-lamp work where the breath
is slice math).

Implementation: `lp-core/lpc-engine/src/nodes/output/output_node.rs`
(`parse_highlight`, `paint_chase`); the producer that emits ordered,
direction-carrying spans lands with the same plan's P2 — until then the
existing producer's sorted-and-merged spans keep meaning breath.

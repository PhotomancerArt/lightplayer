# ADR: map2d format 2 — rotational repeat, inert path gaps, and the loud-refusal format posture

- **Status:** Accepted
- **Date:** 2026-08-05
- **Deciders:** Photomancer
- **Supersedes:** None — extends
  [2026-07-27-map2d-document-architecture](2026-07-27-map2d-document-architecture.md)
  (the document stays an opaque, format-versioned asset; this ADR is the
  first exercise of that format gate).
- **Superseded by:** None

## Context

Bob Zook's 16' Burning Man dome (`examples/zook-dome`, ~1500 LEDs as five
channels × 300 on one classic ESP32) forced three authoring questions the
format-1 schema could not answer:

- A physical channel is one fixed-pitch strip **cut at hubs and jumpered**:
  the polyline has segments that carry no lamps. Format 1 forced one object
  per lit stretch (the dome authored as 10 objects for 5 channels), which
  splits what is physically one strand and makes every hand tweak a
  multi-object edit.
- The dome is five-fold rotationally symmetric — five hand-drawn sectors
  that measure as exact 72° copies of one another to within drawing slop
  (≤9 units over a 1163-unit canvas, verified by
  `scripts/zook-dome/convert.py --repeat`). Authoring five copies by hand
  loses the symmetry the moment one sector is tweaked, and the mapping is
  not final: live symmetry is how the dome's builders will iterate.
- Old builds (device firmware and Studio alike) will meet documents written
  by newer builds. The failure direction had to be decided, not left to
  whatever serde happens to do.

## Decision

1. **`PathShape.gaps: Vec<u32>`** — indices of inert segments (segment `i`
   runs `points[i] → points[i+1]`). Lamps distribute evenly over **active**
   arc length only, and an inert segment emits **no** lamp entries, so
   downstream wiring indices are unshifted. One physical channel is one
   object again. `skip_serializing_if` keeps gap-free documents
   byte-identical to their format-1 selves.

2. **`Map2dShape::Repeat`** — `{ shape: Box<Map2dShape>, center: [f32; 2],
   count: u32 }`, externally tagged like every other variant (the firmware
   graph admits no serde `tag`/`untagged`/`flatten`). Instance `k` is the
   inner shape rotated `k * 360/count` degrees clockwise (screen
   coordinates, y-down); instance 0 is bit-identical to the unrotated inner
   shape. **Each instance resolves to its own span**: instances are
   physical strands, so the engine bridge, the fixture's honest spans, the
   output face's strip boundaries, and the client layout fallback all see N
   strands without special cases. `Rotation2d` in `lpc-mapping` is the
   single home of the rotation arithmetic, so the editor's
   expand-to-instances lands on the same floats the resolver produced.
   Expand exists alongside the first-class repeat for on-playa tweaks —
   symmetry is authored, baking it is a deliberate, reversible editor op.

3. **`MAP2D_FORMAT` bumps to 2, with minimal-format stamping.**
   `Map2dDoc::required_format()` walks the content and returns the lowest
   format the features actually used require; every writer stamps it via
   `normalize_format()`. A plain grid/ring/path document keeps `format: 1`
   and stays readable by every build ever shipped. Only documents using
   `gaps` or `repeat` declare format 2.

4. **Old builds fail loudly on new data; new builds read old data forever.**
   `Map2dDoc::from_json` peeks the `format` field **before** the full
   parse, so a format-3 document on a format-2 build refuses with an honest
   "needs a newer LightPlayer" instead of dying on an unknown variant
   halfway through serde. This direction is deliberate: silent misrender —
   an old build parsing `gaps` as an unknown field and putting lamps on
   jumper wire — is exactly the failure mode format versions exist to
   prevent. ("That's the right kind of failure.")

5. **Editors refuse-to-edit and never rewrite documents they cannot parse.**
   Both editor hosts (the `#/mapping` page's autosave slot and the fixture
   face's asset path) park the editor over an unreadable or newer document
   and leave the bytes exactly as found; the only overwrite path is an
   explicit user action. Aligned with the share-envelope posture: version +
   refuse, never migrate in place.

6. **Reference imagery stays out of the document.** The editor's tracing
   layer (`ReferenceImage`) is host-side state under its own localStorage
   key. Asset bodies budget 10 KiB; reference art (the dome's own source
   SVG is 20 KB) would blow it, and the reference is authoring scaffolding,
   not mapping truth.

## Consequences

- The dome ships as **one gapped sector × repeat 5** and resolves to the
  same 5×300 span structure the per-channel form produced, with uniform
  pitch across each whole channel (the per-channel form's largest-remainder
  split was the approximation; the gapped form is the physical truth).
- A subtlety worth remembering: repeat turns clockwise, and the Zook
  sketch's sectors run counterclockwise in channel order, so repeat
  instance `k` is physical channel `(1,5,4,3,2)[k]` and the example's
  `output.json` lists pins in instance order. Span-per-instance made this a
  pin-table permutation instead of a schema problem.
- Future variant additions repeat this pattern: new variant → walk it in
  `shape_required_format` → old builds refuse loudly with the same honest
  message, no per-variant compatibility code.

## Alternatives considered

- **Repeat as an editor-only bake** (generate N objects on save): no format
  bump, but the symmetry dies on first save — tweaking one sector orphans
  the other four, and the live-symmetry iteration loop (the reason the
  dome's builders wanted this) never exists. Rejected by the product call;
  expand-to-instances preserves the bake as an explicit op.
- **`gaps` without a format bump** (additive field, old builds ignore it):
  old builds would parse the document and distribute lamps over the whole
  polyline — lamps on jumper wire, silently wrong on hardware. The worst
  failure mode. Rejected.
- **Placeholder lamps in gaps** (emit entries, mark them dark): keeps
  counts stable but shifts every downstream wiring index and burns channel
  budget on lamps that do not exist. Rejected.
- **Reference image in the document**: breaks the 10 KiB asset budget and
  entangles authoring scaffolding with mapping truth. Rejected.

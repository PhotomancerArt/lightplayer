---
status: carried
since: 2026-07-31
logged: 2026-08-02
area: lpc-model fixture mapping slots + lpc-engine fixture node + output node
related:
  - lp-core/lpc-model/src/nodes/fixture/mapping.rs
  - lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs
  - lp-core/lpc-engine/src/nodes/output/output_node.rs
  - ../adr/2026-08-01-esp32v3-flash-budget.md
  - s3-frame-cost-scales-per-fixture.md
  - per-frame-optimisations-are-unpriced-in-ram.md
---
# A lamp's position and its colour are each stored two or three times over

**Shape** — the classic ESP32 spends ≈89.5 B of heap per LED, and that number —
not channel count, not flash — is what caps the product claim. (Post-load
headroom was ~18 KB of a 112,640 B arena when this was filed; see the
denominator caveat under Carrying cost.) Attributed on 2026-08-02 with
`lp-cli profile --collect alloc --mode all`, diffing the live-allocation set of
`quad-strips-v3` (120 LEDs) against `quad60-v3` (240 LEDs) by callsite:

| B/LED | Owner | What it holds |
|------:|-------|---------------|
| 25.6 | `mapping_from_map2d_doc` | resolved `MappingConfig::PathPoints` |
| 16.0 | `ensure_direct_points` | `direct_points` (now 12.0, see below) |
| 8.0 | `create_sample_points` | pixel-space coords in graphics memory |
| 8.0 | `create_sample_out` | RGBA16 sample results |
| 6.0 | `OutputNode::consume` | `control_samples: Vec<u16>` |
| 6.0 | `publish_channel_buffer` | runtime buffer bytes — a second copy of the above |
| 21.0 | `DisplayPipeline` | prev/current/next + dither carry (hardware only) |

Two duplications dominate, and both are structural rather than accidental:

**A lamp's position is stored three times.** The authored map2d document
resolves into `MappingConfig::PathPoints { paths: MapSlot<u32, EnumSlot<PathSpec>> }`
where each `PathSpec::PointList` holds `points: MapSlot<u32, XySlot>`. That is a
`Vec<(u32, WithRevision<Xy>)>` — **24 B per lamp to carry 8 B of coordinate**,
because every lamp position is individually revision-tracked and individually
addressable as a slot. Nothing binds, animates, or edits a single lamp
coordinate; the whole point list is replaced atomically when the map2d document
changes. That same position is then derived into `direct_points` (12 B/LED) and
derived again into the graphics `sample_points` buffer (8 B/LED).

**A lamp's colour is stored twice.** `OutputNode::control_samples` is the
`&mut [u16]` render target; `publish_channel_buffer` then copies it verbatim
into the runtime buffer's `Vec<u8>` as little-endian pairs. 6 B/LED each.

Sibling of
[`per-frame-optimisations-are-unpriced-in-ram`](per-frame-optimisations-are-unpriced-in-ram.md),
and the distinction matters: that entry is about *caches* traded for cycles
whose byte cost nobody prices. This one is not a cache. Nothing here was added
to make a frame faster — the duplication falls out of how a lamp is *modelled*,
and it would cost the same bytes if the engine never cached anything.

**Why it is structural** — the per-lamp slot modelling is not a local choice in
the fixture node. `MappingConfig::PathPoints` is wire-visible, studio-visible
and authorable: it appears in `lpc-wire`'s overlay mutation path, in
`lpc-registry`'s inventory derivation and base-value display, and in the
studio's slot controller and composite slot UI — roughly twenty files. Packing
the point list into a blob is a schema change to on-disk project files and to
the slot-editing surface, not a representation tweak, which is why it did not
land with the measurement that motivated it.

**Carrying cost** — 25.6 B/LED for the slot-modelled point list and 6 B/LED for
the duplicated colour copy is **31.6 B/LED, over a third of the per-LED
budget**, on the one chip where the budget binds.

Deliberately not converted into an LED ceiling here. The ceiling is
[LED count × shader size, not LED count alone](../adr/2026-08-01-esp32v3-flash-budget.md),
and the arena moved underneath this entry while it was being written: PR #288
(JIT code region right-sizing) took the classic from 112,640 B to **178,176 B**.
A per-LED saving is a real saving at any arena size; an LED ceiling quoted
against a stale arena is just wrong.

⚠️ **And per-LED is not the dominant term at realistic scale.** Comparing two
projects with near-identical LED counts but different node counts —
`examples/basic` (241 LEDs, 1 fixture + 1 output, ~58 KB of project cost) vs
`quad60-v3` (240 LEDs, 4 + 4, ~100 KB) — puts roughly **14 KB on each
fixture+output pair**, against ~21 KB of per-LED cost for the whole 240-LED
project. On a four-channel show, node cost already exceeds LED cost by 2×.
(Cross-image comparison, so treat 14 KB as approximate; the magnitude is not in
doubt.) For the real target — **1500 LEDs on four channels** — that is ~78 KB
gone before a single LED, leaving ~96 KB of the 178,176 B arena, i.e. a budget
of ~64 B/LED just to fit and less once the compile transient is counted. **The
per-node cost deserves the same alloc-diff treatment this entry gave per-LED,
and nobody has done it.**

## Paying it down

Three separable steps, cheapest first:

1. ~~**`direct_points` keeps only the channel** (12 → 4 B/LED).~~ **PAID DOWN
   2026-08-03** (PR #303): `direct_channels: Vec<u32>` is the only resident
   per-lamp state; coordinates regenerate transiently when the sample-point
   buffer's `(mapping_version, width, height, count)` key changes — which
   also killed an every-frame coords-Vec alloc + buffer rewrite (8 B/LED of
   transient churn per frame). The key lives inside the handle's Option so a
   memory-pressure drop invalidates both atomically; a rewrite-only-on-key
   test pins both behaviours. Measured: frame retained −1,928 B at 241 LEDs
   (exactly 8 B/LED).
2. ~~**Render straight into the runtime buffer** (−6 B/LED).~~ **PAID DOWN
   2026-09-02** (plan `2026-09-02-2154-per-lamp-memory-table`): the runtime
   buffer stores output channels as `u16` (`RuntimeBufferData::Samples16`),
   the output node takes that storage for the frame and hands it back at
   publish, and the flush borrows it — `control_samples`, the LE byte copy,
   and the flush-side decode (`flush_samples`, the "third leg") are all gone:
   −12 B/LED resident per output and two copies per frame, with the control
   render target contract untouched. The same pass removed a copy this entry
   never counted: `FixtureNode::sampled_scratch`, an 8 B/LED CPU copy of the
   RGBA16 sample-out refilled every frame, replaced by an in-place borrow
   (`LpGraphics::sample_out_data`).
3. ~~**Pack the resolved point list** (−17 B/LED).~~ **PAID DOWN for
   document-sourced fixtures 2026-08-04** (M6, branch
   `claude/m6-compact-mappings-f98a49`): the split held exactly as framed —
   map2d/svg-sourced mappings resolve into `ResolvedMappingCompact`
   (~8 B/lamp exact-capacity spans+points in lpc-model, non-slot,
   non-serialized) while hand-authored `PathPoints` keeps the slot form and
   its editing surface. The *actual* saving was bigger than the estimate:
   the slot form's live cost was 41 B/LED (power-of-two `VecMap` overshoot;
   61,440 B at 1500 lamps), so the dome's resolved mapping went
   61,440 → ~9 KB. The same change deleted a per-tick whole-mapping clone
   (36 KB/frame at 1500) and the contiguous `Vec<MappingPoint>` expansion
   (38,400 B doubling peak = the first-tick OOM). Silicon: 1500 lamps runs
   at 12 fps, used=137,332 of 186,368; post-M6 marginal ≈ 59 B/LED all-in.

## Incident log

- **2026-08-02** — attributed. Prior to this the engine-side ~68 B/LED was
  recorded in the flash-budget ADR as "unattributed and the single most
  valuable RAM lead this chip has", and was believed to scale with `render_size`.
  It does not, on the direct-sampling path: it scales with mapped lamp count.
- **2026-08-02** — the `render_size` multiplier measured separately, on the
  `TextureArea` path (`examples/fast`, 16×16 canvas, **one** lamp): 1,024 B of
  `PixelMappingEntry` (4 B/canvas pixel) + 2,048 B of RGBA16 render target
  (8 B/canvas pixel) = **3,072 B per fixture for a single LED**, i.e. 12 B per
  canvas pixel *independent of lamp count*. 34× what a direct-sampled LED costs.
  Only `examples/fast` uses this path today, so it contributes nothing to the
  89.5 B/LED figure — but nothing in the authoring surface tells an author that
  widening `render_size` on a texture-area fixture is a RAM decision. Worth its
  own entry if that path ever ships.
- **2026-08-02** — 13 B/LED of the 89.5 paid down in #285 (conditional
  `DisplayPipeline` buffers, 9; `direct_points` right-sizing, 4). The three
  steps above are what remains.
- **2026-09-02** — measured per owner at two lamp counts on the host and on
  the emulator (`docs/reports/2026-09-02-per-lamp-memory-table.md`): before,
  49 B/LED device-side engine residents (+8 mapping at load, +6–21 classic
  `DisplayPipeline`); after step 2 + the sample-out borrow, 29 B/LED
  (mapping 8, direct channels 4, sample points 8, sample target 8, output
  samples 6, 8-bit frame 3 — every one a home, no copies). Load peak 32 →
  8 B/LED: the map2d resolver streams into the carrier's own buffer and the
  loader no longer resolves the document twice. zook emulator `frame`
  retained 82,864 → 52,864 B; the classic reads ≈43 B/LED all-in with
  interpolation off. The remaining lamp-scaled copy is the classic's
  `DisplayPipeline.current` (6 B/LED, +12 with interpolation).
- **2026-08-03** — step 1 paid down in PR #303 (−8 B/LED resident, −8 B/LED
  per-frame transient churn). Step 2 is deferred while PR #301's P2 rewrites
  the flush path it would touch; the u16→u8 copy it targets gained a third
  leg worth collapsing with it (flush decodes the bytes back to u16
  per frame). Step 3 remains the schema question.

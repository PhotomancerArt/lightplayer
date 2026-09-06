# ADR: Direct sampling streams through a bounded batch — the mapping is the coordinates' only home

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-06-0246-small-dome-sample-batches` (PR #527)
- **Supersedes:** None
- **Superseded by:** None
- **Related:** `docs/adr/2026-09-06-control-render-targets-scatter.md` (the
  previous per-lamp transient); `docs/reports/2026-09-06-small-dome-first-frame-budget.md`
  (the attribution); `docs/reports/2026-09-02-per-lamp-memory-table.md`
  (the owner table); `docs/heap-budget-gate.md`

## Context

A Direct fixture sampled its visual by holding two graphics buffers sized
to the product — `count` Q16 sample points (8 B/lamp) and `count` RGBA16
results (8 B/lamp) — and, whenever the coordinate key changed, generating
the coordinates into an exact-capacity transient `Vec<i32>` (another
8 B/lamp) before uploading them. On `examples/small-dome` (5,950-lamp dome +
360-lamp doors) the first frame halted the 320 K RV32 emulator guest on
that transient: the 47,600 B sample-points buffer had just been allocated,
the 47,600 B coordinate Vec was the ask that failed with 12,445 B free, and
the 47,600 B sample target was still to come. Attribution
(`docs/reports/2026-09-06-small-dome-first-frame-budget.md`) showed the
frame needed ~415 KB of a 327,680 B heap before any shader compile: no
allocation order fixes it, only fewer per-lamp residents do. Of the Direct
path's 33 B/lamp on a device, 16 were these two buffers.

The lpvm call boundary already took an explicit point `count`, and the wgpu
sample pass was slice-driven, so sampling a *prefix* of a buffer was
natural on every backend. What was not: uniform binding — the part of a
sample that allocates (uniform paths formatted per member on the CPU tier,
a bind group on the GPU tier) — happened inside every sample call, so a
naive "sample in chunks" loop would have multiplied it by the chunk count.
And the coordinates had no resumable source: `for_each_mapping_point`
visited the whole mapping in one call.

## Decision

**A visual product is sampled through a bounded stream, driven by the
producer.**

- `lp_gfx::LpShader` splits binding from sampling: `bind_uniforms(&uniforms)`
  once, then `sample_rgba16_bound(points, out, count)` for the first `count`
  points of a window, as many times as the stream needs. `sample_rgba16`
  stays as the one-shot composition for tests and parity checks.
- `lp_gfx::LpGraphics::sample_batch_capacity()` says how many points a
  backend wants per call: `u32::MAX` by default (one call per product — on
  a GPU a call is a device round trip, so batching costs more than it
  saves), **128** on every CPU engine (`lp_gfx_lpvm::CPU_SAMPLE_BATCH_POINTS`;
  a call there is a function call, and 128 points is 1 KB in, 1 KB out,
  ~8 calls per 1,000 lamps).
- The engine's sampling request is `VisualSampleStream`: the consumer owns a
  **window** — a points handle, a sample-out handle, and a host-side
  coordinate scratch, all of one capacity `min(lamps, sample_batch_capacity)` —
  and two closures. The producer drives the loop (`VisualSampleStream::drive`):
  `fill` writes the next batch of coordinates into the scratch, the producer
  uploads it, samples the first `n` points bound, and hands `n × 4` RGBA16
  words to `consume` through the in-place `sample_out_data` borrow. Nothing
  sized to the product exists on either side.
- The shader node binds once per stream, keyed on `(output_width,
  output_height, time bits)` and reset by `produce`, texture renders and
  recompiles, so the playlist crossfade's per-batch one-batch inner streams
  bind once per frame per entry. Projection (a request whose space differs
  from the shader's) maps each batch host-side from the stream's own scratch
  into a window-sized resident scratch — no read-back, no per-frame Vec.
- The fixture's coordinates come from `lpc_model::nodes::fixture::mapping_centers`,
  a resumable iterator over the mapping in the visitor's order (pinned
  against `for_each_mapping_point`), converted per batch to pixel-space
  Q16. The mapping is the coordinates' one home; the window is a borrow.
- The playlist crossfade drives the outer loop itself: per batch it samples
  the outgoing entry into its one window-sized sample-out, blends the
  incoming entry's answer over it in place, and hands the blend on. The
  fluid node samples each batch on the CPU into a window-sized scratch.
  The module node forwards.

This is the #495/#497/#503/#523 rule applied to the last two per-lamp
graphics residents: **the thing with one home stores the value; a consumer
borrows a window.**

## Consequences

- **Per-lamp residency on every device drops by 16 B/lamp** (Direct path:
  33 → 17 B/lamp; mapping 8 at load + output samples 6 + 8-bit frame 3),
  and the 8 B/lamp coordinate transient at first render is gone. Each
  Direct fixture with ≥ 128 lamps holds a 3 KB window instead
  (`3 × 1,024 B`); smaller fixtures size the window to their count, as they
  sized the buffers before. On small-dome that is −100,960 B resident and
  −47,600 B transient in the first frame; on zook −24,000 + 3,072 B. The
  measured device-width figures are in the plan's report ("After").
- **Coordinates are regenerated every render** — the per-lamp cost the
  window buys its bytes with. `normalized_f32_to_q16` is integer bit-exact
  (P4 of the plan) so a part with no FPU pays ~15 integer ops per
  coordinate, not four libcalls; the measured cycles/frame delta on zook is
  in the report. A product cut across two outputs regenerates twice (it
  rendered twice already — scatter ADR follow-up).
- **No per-frame O(lamps) allocation on any sampling path.** The projected
  path lost its `read_sample_points` + `mapped` Vecs, the crossfade its two
  count-sized sample-outs, the fluid node its two per-frame Vecs. The
  crossfade's transition residency is one window + one blend scratch.
- **Bytes are identical.** Per-point shader evaluation cannot see its batch;
  `output_control_samples_golden`, `output_scatter`, `output_patch_reflow`
  pass unedited, and `tests/direct_sampling_batches.rs` renders eight
  examples and a mid-transition crossfade at capacities 1 / 7 / 128 /
  unbatched and requires the published bytes to agree.
- A batch that traps mid-stream leaves the earlier batches' lamps in the
  target before the output's fault pattern paints over them — the same
  "over whatever extent the frame established" rule a failed render already
  followed; the fuel report carries the batch's first product sample.
- Decorators over `LpGraphics` / `LpShader` must forward the three new
  methods. One that forgets `sample_batch_capacity` answers the default and
  silently un-batches its inner backend: bytes stay right, memory does not.
- The `per_lamp_memory_table` and `playlist_crossfade_memory` probes are
  re-pinned to the window (their old pins measured the count-sized
  buffers).

## Alternatives Considered

- **Quantize the sample-points buffer** (u16 per coordinate): pixel-space
  Q16 for a 128-wide target needs 24 bits, and normalized Q0.16 cannot hold
  1.0. Not byte-identical.
- **Allocate the buffers at load**, ahead of the frame's transients: the
  halting ask was the coordinate transient, and the frame was ~90 KB over
  the heap regardless of order.
- **Store Q16 in the mapping and drop the f32 points**: a format change to
  `ResolvedMappingCompact`, lossy for the TextureArea precompute and Studio
  layouts — not byte-identical outside the Direct path.
- **Per-output sampling of only this output's runs**: the buffers key on
  count, so two outputs would reallocate every frame; the render-whole
  contract (#523) stands.
- **Chunk in the consumer with the existing one-shot API**: binds uniforms
  per chunk (an allocation per member per chunk) — the churn the
  steady-render ratchet exists to catch.
- **Stream the coordinates but keep the sample target whole**: −8 B/lamp,
  not enough — the target's 47,600 B was the next ask with 12 KB free.

## Follow-ups

- One engine-wide window instead of one per fixture (3 KB × fixtures today).
- The emulator's fixed overheads a device never pays — the 256-resource
  manifest (36,864 B resident, 20,480 B per port open) and the in-RAM
  `LpFsMemory` deploy (~30 KB on small-dome) — are now ~1/5 of the guest
  heap on that project.
- A per-frame render cache on the producer so a product cut across outputs
  samples once: with the window, "render once" would need a per-lamp home
  again — a real trade, not a free follow-up.
- `write_direct_lamps` (~313 cycles/lamp on the C6 model: gamma, brightness
  and power per channel) is now the largest non-shader per-lamp cost.

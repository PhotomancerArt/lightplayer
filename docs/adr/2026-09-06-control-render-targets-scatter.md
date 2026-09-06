# ADR: Control render targets scatter — the runtime buffer is a rendered product's only home

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-06-0048-output-scratch-resident` (PR #523)
- **Supersedes:** None
- **Superseded by:** None
- **Related:** `docs/reports/2026-09-02-per-lamp-memory-table.md` ("Still
  per-lamp and per-frame"); `docs/defects/2026-08-29-flash-write-wedges-under-zook-playback.md`
  (the per-frame alloc+free shape this removes another instance of);
  `docs/heap-budget-gate.md`

## Context

An output renders N producers into disjoint sub-slices of its runtime
buffer. A producer with no patch contributes its whole product as one run,
and renders straight into that sub-slice through a contiguous
`ControlRenderTarget` — byte-pinned by `output_control_samples_golden`.

A patched producer is cut into several runs of its own lamps, each landing
somewhere else on the wire (possibly on a different output). Until this
decision, `OutputNode::render_fragments_into` handled a partial run by
rendering the producer WHOLE into a fresh `Vec<u16>` (`ProductScratch`,
`alloc::vec![0u16; extent.sample_count()]`, 6 B per lamp of the product,
once per product per frame) and copying each run out of it. The per-lamp
memory table measured it: `examples/small-dome` (a 5,950-lamp dome and a
360-lamp door, both patched) paid a 42,278 B host transient every steady
tick, and on the RV32 emulator its first frame died on exactly that ask —
`Guest halted: OOM (size 35700)` = 6 × 5,950 — after #503 had moved every
other per-lamp transient into a resident home. On the classic ESP32's
infallible allocator a patched 1,500-lamp fixture would pay 9 KB of
alloc+free per frame, the same shape as the flash-write wedge.

The producer's side of the contract makes the scratch look necessary: every
fixture writer renders the whole product — `fill(0)`, then one write per
lamp at `channel * 3` in mapping order, not sequentially — and the power
pass accumulates demand across that whole render. A slice of a product is
not something a producer can be asked for cheaply.

## Decision

**The target owns placement.** `ControlRenderTarget` has two shapes:

- **Contiguous** (`new`): buffer sample `i` is product sample `i`. Unchanged;
  the path every unpatched project takes.
- **Scattered** (`scattered`): the output's WHOLE runtime buffer plus the
  runs a patch cut the product into (`ControlTargetRun { source_offset,
  offset, len }`). `write(product_offset, values)` lands each sample on every
  run that claims it, at `run.offset + (sample − run.source_offset)` — the
  address the copy used to put it at. `clear()` zeroes exactly the run
  regions. Product samples no run claims are dropped.

The producer's contract does not change: it still renders its whole product,
once per output per frame, through `product_len()` / `clear()` / `write()`,
and never learns where the samples land. `samples` is private, so a writer
cannot bypass placement. Reversal and rotation within a run stay post-passes
the output applies in place after the render, exactly as when the run was a
copy.

The output renders product by product: the first time a product appears in
the fragment set it renders once — contiguous when its single fragment
covers the whole product, scattered otherwise — and spans are then placed in
fragment order. The runs live in one resident `Vec` on the output node,
sized fallibly (`ensure_scratch_len`), so after the first frame at a given
run count the patched path allocates nothing.

This is the #495/#497/#503 rule applied to the last per-lamp transient on
the tick path: **the thing with one home stores the value; a node borrows.**
A rendered product's samples have one home — the output's runtime buffer —
and nothing is materialized beside it.

## Consequences

- No O(lamps) allocation on the patched path. small-dome's steady tick
  transient drops from 42,278 B to the O(runs) bookkeeping; the 35,700 B ask
  that halted the emulator is gone. A patched product costs a run lookup per
  write (binary search over the product's runs when they are source-disjoint
  — ~5 compares per lamp for the dome's ~26 runs per output; a linear scan
  only when a patch lists the same lamps twice).
- Bytes are identical. The writes land where the copy landed them; the
  zeroing covers what the copy overwrote; skipped-fragment rules (a run that
  does not fit the buffer, or reaches past its product) are unchanged, and
  so is the gap/contested zeroing that makes render order irrelevant.
- `render_control` is called once per product per output per frame, as the
  scratch cache already did. A product with a whole-covering fragment AND a
  second, partial one (duplicate lamps) rendered twice before and renders
  once now; its bytes are the same (the overlap is contested, hence zeroed).
- Every writer goes through the API. A new producer that wants to write
  samples has one way to do it, and it is placement-correct by
  construction.
- `project_read_probes` keeps rendering a product into its own Vec through
  the contiguous target — a read, not the tick path.

## Alternatives Considered

- **A resident scratch on the output node** (`ensure_scratch_len`): cheapest
  change, but it keeps 6 B/lamp resident per patched product PER OUTPUT the
  product lands on (small-dome: 2 × 35,700 B), and it still asks the
  emulator for the 35,700 B that halts it — fallibly, so the halt becomes a
  degraded frame, never a baseline. It moves the cost; it does not remove
  it.
- **Per-run render** (the producer renders only `[a, b)`): removes the
  buffer, but a fixture would either re-sample the visual per run (26× the
  shader work for the dome) or cache its sampling per frame, and the power
  pass's whole-product demand would have to be split across runs. Invasive
  on the producer side for the same result the target can deliver alone.
- **Render whole into the first run's output region, copy the rest**: zero
  extra buffer only when one output's buffer holds the whole product;
  small-dome's dome is split across two outputs, each smaller than it, so
  this needs the resident scratch as its fallback.

## Follow-ups

- Each output that takes runs of a product still renders it whole
  (small-dome renders the dome twice per frame). A per-frame render cache on
  the PRODUCER — the product's own home — would render once for every
  output; a different plan.
- The O(runs) per-frame Vecs in the output's render path (products,
  placements, fragments, spans, per-product layouts) are small and could
  become resident scratches on the node.

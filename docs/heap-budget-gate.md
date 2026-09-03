# The heap-budget ratchet gate

`just heap-budget-check` measures per-window heap budget figures for the
projects listed in `scripts/heap-budget-record.json` by running
`lp-cli profile --collect alloc` on the RV32 emulator — once in `--mode startup`
and once in `--mode steady-render` per project — and fails if any figure grew
beyond the recorded value (default margin 0%).

It exists because memory regressions in this repo have landed silently and
been found weeks later on hardware: `main` drifted +3,736 B of loaded cost
with nobody noticing, and #243 cost the classic ~8.3 KB and a full day of
hardware bisecting. Every one of those was measurable on the host the day it
landed.

## What is measured

Two profile modes per project:

- **`startup`** — project load through the first *compiled* frame. Every
  perf-event window in the trace is recorded: `project-load`, `shader-compile`,
  `shader-link`, `frame`. Its `frame` window brackets frames 1–2, and frame 2
  contains the shader compile (compiles are deferred one frame; see
  `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`), so this
  `frame` is a **cold-start** figure.
- **`steady-render`** — two warm-up frames, then four captured steady frames.
  Only the `frame` window is recorded (the other windows lie outside the
  capture and would record zeros). This is the **steady-state** figure and the
  home of the per-frame allocation ratchet.

Five figures per window:

- **transient** — peak live bytes above the window's live-at-open baseline,
  maximised across openings. The cost of doing whatever the window does.
- **retained** — live bytes at close minus at open, maximised across
  openings, floored at zero. What the window leaves resident.
- **largest_alloc** — largest single allocation request inside the window.
  The proxy for contiguity failures (see limits below).
- **alloc_count** — allocation requests (allocs + reallocs) inside *one*
  opening of the window, maximised across openings. For `frame` that is the
  worst frame, not the sum over however many frames the gate captured.
- **alloc_bytes** — bytes requested inside one opening, maximised across
  openings. Churn, not residency: a steady frame that allocates and frees the
  same 25 KB every time costs nothing in `retained` and everything here. On a
  part with a linked-list allocator, churn is cycles (malloc + free were 19%
  of Meteor's frame on 2026-09-02) and fragmentation pressure.

⚠️ **Per-LED cost lands in the `frame` window, not `project-load`.**
`direct_points`, the graphics sample buffers and `DisplayPipeline` are all
allocated at tick/output-open time, so a per-LED regression shows up as
`frame.retained` growth. A project-load bracket does not capture it.

### The record

```json
{
  "projects": {
    "examples/basic": {
      "modes": {
        "startup":       { "windows": { "project-load": {…}, "shader-compile": {…}, "shader-link": {…}, "frame": {…} } },
        "steady-render": { "windows": { "frame": { "transient": …, "retained": …, "largest_alloc": …, "alloc_count": …, "alloc_bytes": … } } }
      }
    }
  }
}
```

`check` compares every figure the record holds, generically: a figure added to
the record ratchets from then on. A recorded window missing from the
measurement fails (the instrument or the instrumented path broke).

To add a project, add its key under `projects` (an empty object is enough)
and run `just heap-budget-baseline`; the baseline reads the project list from
the record. Recorded today: `examples/basic` (the smallest real project) and
`examples/meteor` (a compute-shader project with a struct-valued map slot —
the per-frame churn case).

## Ratchet, not ceiling

The record holds **today's measured values** — descriptive ("what this
project costs today"), not prescriptive ("what the dome may use"). Any growth
fails; an intentional increase updates the record in the same PR:

```bash
just heap-budget-baseline
```

which regenerates `scripts/heap-budget-record.json` from the current tree, so
the growth appears in the PR diff where a reviewer sees it. Same shape as
`just fw-esp32v3-size-check`, with one difference: that gate compares against
a real limit (the partition size); this one compares against last-measured.

The margin defaults to **0%** — the emulator is deterministic (simulated
time, no host randomness), so identical trees produce identical figures. If
noise ever appears, that is itself a finding, not something to widen the
margin over. **Never widen the margin to make the gate pass.**

## Why deltas, not absolutes

The guest heap (`lp-riscv/lp-riscv-emu-guest/memory.ld`, `HEAP_SIZE`) is
deliberately **not** the device arena. Measured 2026-08-02: the guest carries
~52 KB of harness baseline the firmware does not (63,596 B live at
project-load start vs ~10,936 B idle on a classic ESP32), so a device-sized
heap would OOM the emulator on projects the device runs comfortably. What
transfers is the **deltas**: project-load cost measured 51,723 B on the
emulator vs 53,052 B on the classic — within 2.6%.

## Fidelity limits — what this gate cannot see

A harness that overstates its fidelity is worse than none. This gate does
**not** model:

- **Fragmentation.** The figures are live-byte accounting; the emulator's
  allocator differs from `esp_alloc`, so arena layout and fragmentation
  behaviour differ. A workload can pass this gate and still fail on device
  because the arena is fragmented.
- **Two-region arenas / contiguity.** The guest heap is a single region. The
  classic's post-#288 arena is two regions, where a large allocation can fail
  while total free is ample. The `largest_alloc` ratchet is the proxy: it
  catches a *new* big contiguous ask, not a layout change that makes an old
  one stop fitting.
- **The classic's resolver configuration.** `fw-emu` runs the engine as the
  reference target does, with `resolver-payload-cache` **on** (since
  2026-09-02; `fw-esp32c6` and `fw-esp32s3` match). The classic `fw-esp32v3`
  runs decisions-only to save arena
  (`docs/debt/per-frame-optimisations-are-unpriced-in-ram.md`), which
  re-materialises every unbound default each frame: its `frame` churn is
  higher and its `frame.retained` lower than these figures. What the gate does
  price is the payload table itself — turning the cache on moved
  `examples/basic` `frame.retained` by the bytes the table costs, in the record
  diff — which is the byte number that debt entry asked for.
- **Stack usage.** Neither RV32 nor Xtensa stack consumption is modeled at
  all.
- **The JIT code region.** The emulator covers the heap;
  `lp-shader/lpvm-native/tests/xt_classic_codemem_corpus.rs` covers the code
  region (it predicts device JIT size byte-exactly — 5 silicon matches, 0
  misses). Neither gate covers the other's territory.
- **Xtensa anything.** The guest is RV32. Per-LED and compile-transient
  deltas have transferred well to the classic in practice (2.6% above), but
  that is measured correspondence, not emulation.

## CI

Runs in the `Validate (x64)` job of `.github/workflows/pre-merge.yml` when
core paths changed — four emulator runs (two projects × two modes), after the
tests so `lp-cli` and `fw-emu` reuse warm dependencies. Referenced from
`docs/adr/2026-08-01-esp32v3-flash-budget.md`.

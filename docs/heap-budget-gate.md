# The heap-budget ratchet gate

`just heap-budget-check` measures per-window heap budget figures for the
projects listed in `scripts/heap-budget-record.json` by running
`lp-cli profile --collect alloc --mode startup` on the RV32 emulator, and
fails if any figure grew beyond the recorded value (default margin 0%).

It exists because memory regressions in this repo have landed silently and
been found weeks later on hardware: `main` drifted +3,736 B of loaded cost
with nobody noticing, and #243 cost the classic ~8.3 KB and a full day of
hardware bisecting. Every one of those was measurable on the host the day it
landed.

## What is measured

For each perf-event window in the trace (`project-load`, `shader-compile`,
`shader-link`, `frame`), three figures:

- **transient** — peak live bytes above the window's live-at-open baseline,
  maximised across openings. The cost of doing whatever the window does.
- **retained** — live bytes at close minus at open, maximised across
  openings, floored at zero. What the window leaves resident.
- **largest_alloc** — largest single allocation request inside the window.
  The proxy for contiguity failures (see limits below).

⚠️ **Per-LED cost lands in the `frame` window, not `project-load`.**
`direct_points`, the graphics sample buffers and `DisplayPipeline` are all
allocated at tick/output-open time, so a per-LED regression shows up as
`frame.retained` growth. A project-load bracket does not capture it.

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
core paths changed. Referenced from
`docs/adr/2026-08-01-esp32v3-flash-budget.md`.

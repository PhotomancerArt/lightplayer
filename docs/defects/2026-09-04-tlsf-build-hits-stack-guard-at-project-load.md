---
status: open
found: 2026-09-04      # how: silicon bench, plan 2026-09-04-1358-classic-heap-fragmentation-research
area: fw-esp32v3 with esp-alloc's TLSF heap (ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF)
class: crash-loop
related:
  - ../reports/2026-09-04-classic-heap-fragmentation.md
  - 2026-08-07-boot-compile-oom-crash-loop.md
---
# The TLSF build of fw-esp32v3 hits the stack-guard watchpoint while loading a project

**Shape** — `fw-esp32v3` built with `ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF`
(esp-alloc 0.10 over `rlsf` 0.2.2; verified by `llvm-nm`: `rlsf` symbols
present, `linked_list_allocator` absent; image +5,744 B). Flashed to the
desk classic (DOM-Z-102), it panics during the startup project's auto-load,
three boots in a row, until the recovery ledger disables the project:

```
====================== PANIC ======================
Detected a write to the stack guard value on ProCpu
PC 0x40210ebc  lpfs::lp_path::normalize
A0 0x402148d2  <lpfs::lp_fs_view::LpFsView as lpfs::lp_fs::LpFs>::read_file
```

(`just decode-backtrace-esp32v3` against the TLSF ELF, kept as
`bench/fw-esp32v3-tlsf` in the planning directory; full serial log in
`bench/bench-tlsf.csv.log`.) The first-fit build from the same tree boots
and loads the same project on the same board.

**Two hypotheses, not yet separated:**

1. **Pool overlaps the guard.** `heap_allocator!(size: HEAP_SIZE)` places the
   arena as a static in `dram_seg`, adjacent to `.stack`; first-fit never
   writes the arena's last bytes (its `Hole` layout rounds the top down)
   while rlsf's `insert_free_block_ptr` lays a sentinel block at the pool's
   end. If esp-hal's guard word sits inside or at the boundary of the arena,
   TLSF writes it during init or on the first allocation that reaches the
   pool's tail — and `normalize` is simply the first writer of a block
   placed there.
2. **Stack depth.** The watchpoint is a stack-overflow detector; the TLSF
   build may run `read_file → normalize` a few hundred bytes deeper than
   first-fit (different inlining), crossing a guard that first-fit only
   grazed. `.stack` and the arena are in zero-sum competition on this chip
   (`HEAP_SIZE` doc comment in `fw-esp32v3/src/main.rs`).

Distinguish them by printing the arena span and the guard address at boot,
and by moving the guard/arena boundary 64 B: hypothesis 1 moves with the
arena, hypothesis 2 does not.

**Also observed** — idle with no project (the ledger having disabled it):
167,024 B free / 88,047 B largest, against first-fit's 170,332 / 94,780.
TLSF's static bookkeeping costs ~3.5 KB of heap and ~6.7 KB of largest
block before any allocation. Worth carrying into the TLSF decision.

**Why it matters** — the report's TLSF row is unranked; this defect is the
reason a "one-line config flip" is not a lever until it boots a project.

## Root cause (2026-10-10, RAM research E6)

**Neither hypothesis as stated: the TLSF control block is a static, and it
comes out of the stack.** esp-alloc's `EspHeap` holds `MAX_REGIONS` (5 in
the fork, `third_party/esp-alloc`) `Option<HeapRegion>` slots, and under
`TLSF` each slot holds a whole `rlsf::Tlsf<'static, usize, usize, 32, 32>`:
32 × 32 list heads of 4 B plus the bitmaps, 4,228 B, so `esp_alloc::HEAP`
grows from 192 B to 21,256 B (`rust-nm -S`). On the classic and the S3 that
static lands in `.data`, on the C6 in `.bss`; on all three the main stack is
what is left above the statics, so it loses the same 21,064 B:

| image (research/ram @ `8733eb97b`) | `esp_alloc::HEAP` | main stack |
|---|---:|---:|
| fw-esp32v3, first fit | 192 B | 37,056 B |
| fw-esp32v3, TLSF | 21,256 B | **16,000 B** |
| fw-esp32v3, TLSF with `FLLEN` 14 | 9,376 B | 27,872 B |
| fw-esp32s3, first fit / TLSF | 192 / 21,256 B | 32,416 / **11,352 B** |
| fw-esp32c6 split, first fit / TLSF | 192 / 21,256 B | 48,984 / **27,920 B** |

The first-fit classic's own load peaks at 30,336 B on that stack
(`projects/test/basic`, `lp-emu:esp32v3:t1`), so both TLSF classic images
hit the guard at the upload's `loadProject`, after the last file write and
before the load logs anything; the stock S3 hits it at boot; the stock C6 (`lp-emu:esp32c6:t1`) at its first
project load (`Detected a write to the main stack's guard value`). No
evidence of deeper recursion: the panics come from interrupt entry with the
stack pointer 16 B above `_stack_end`. Printing the arena and the guard, as
proposed above, is not needed; `rust-nm -S` on the two ELFs is the check.

**Status** — open, because the TLSF build is still not a lever: with the
stack kept (the main heap shrunk by the control block) TLSF measured worse
than first fit on every C6 workload replayed, and its 8 B header and 16 B
granule cost 23–28 KB of live set (E6's report in the planning folder,
`lp2025/2026-10-09-1203-ram-research/experiments/e06-tlsf-vs-first-fit/`).

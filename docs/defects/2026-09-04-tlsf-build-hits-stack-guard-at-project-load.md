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

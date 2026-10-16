---
status: fixed      # this change: third_party/esp-alloc's LLFF region test takes `ptr < top`
found: 2026-10-09  # RAM research E4, lp-emu:esp32c6:t1, on a layout where two heap regions touch
area: third_party/esp-alloc (`src/heap/llff.rs`, `LlffHeap::try_deallocate`) × any firmware that registers two address-adjacent regions, the lower one first
class: off-by-one   # an inclusive end where the interval is half-open
related:
  - ../../third_party/esp-alloc/README-LP.md   # "The second diff"
  - lp2025/2026-10-09-1203-ram-research        # E4 report (experiments/e04-stack-into-dram2)
---
# esp-alloc gives the first block of a region to the region below it

**Symptom** — on an emulated C6 whose main heap region ended exactly where a
second region began (RAM research E4's layout: main up to `0x4086B910`, a
Rust-only region from there), the PLAYFUL Choker's shader edits panicked
within a minute: `Freed node (…) aliases existing hole (…[624])! Bad free?`,
from `linked_list_allocator` under a `Vec<SlotFieldShape>` drop. The
allocation trace shows the main region handing out an 11-byte block at
`0x4086B90C` — straddling the boundary by 8 bytes, over the upper region's
own hole header.

**Cause** — `EspHeap::dealloc` offers the pointer to each region in
registration order, and the LLFF region accepted it when
`bottom <= ptr && top >= ptr`. `top` is one past the region's last byte, so
no block of the region starts there — but the first block of a region whose
span begins at that address does. Freeing it put a hole past the lower
region's end into the lower region's list (the trace shows the free at
`0x4086B910` a few hundred thousand events earlier), and the lower region
then allocated from it. The TLSF region (`src/heap/tlsf.rs`) already used the
exclusive end (`pool_end > addr`).

**Why nothing shipped trips it** — no shipped layout has a lower region that
touches a higher one *and* is registered before it. The C6's radio region
ends where its main region begins, but the main region is registered first
and claims that address by its `bottom`. It is latent: any future layout that
splits a span into two regions in address order (a lender, an arena, a
reclaimed tail) would corrupt the heap the first time the upper region's
first block is freed.

**Fix** — `ptr < top`. One comparison, recorded as the fork's second diff in
`third_party/esp-alloc/README-LP.md`. Verified on E4's layout: the same
choker-edits workload (`scripts/ram/e09-trace-run.py … choker-edits`) ran its
14 compiles and ten edits to the end with zero allocator anomalies in the
trace.

---
status: fixed
found: 2026-10-09      # how: report (RAM research E14, on lp-emu:esp32s3:t1)
fixed: this change
area: fw-esp32s3 recovery::panic_path::largest_free_block
class: config-masked-defect
related: [docs/defects/2026-10-06-the-largest-block-probe-reads-a-64-kib-hole-as-65535.md]
---
# The S3's largest-block probe was optimized away, so it always reported `free`

**Symptom** — RAM research E14 gave the S3 a second heap region (the
bootloader's `dram2_seg`, 73,744 B) beside its 245,760 B arena, and the first
elicited heartbeat on `lp-emu:esp32s3:t1` said
`[MEM] free=287936 used=31564 largest_free=287936` — a largest block bigger
than either region. A probe in the same image that kept its pointer
(`alloc(250_000)` → `0x0`) proved the allocator itself was right.

**Root cause** — `largest_free_block` binary-searches with "allocate this size,
free it at once" as the predicate. Rust lets the optimizer delete an allocation
whose only use is its own deallocation, and with esp-alloc's `GlobalAlloc` as
the global allocator LLVM did exactly that: the shipped image's function
(`fw-esp32s3-shipped.elf` at `fa62f4c6c`, `0x42070d08`) calls `EspHeap::free`
and then runs the search loop with no call at all, so every probe "fits" and the
answer is `free()`. With one region and a fresh heap the two numbers happen to
agree, which is why nothing noticed: the recorded `largestFreeBlock` equalled
`freeBytes` (214,192) and read as plausible. On any fragmented S3 heap the
probe has over-reported, and the S3's heap gates (the load headroom gate, the
read frame budget of half the largest block) read that number. The classic's
copy of the same code survives because its global allocator is a wrapper
(`RetryingHeap`) the optimizer calls through; the C6's survives too
(`p2.elf` at `fa62f4c6c`: `GlobalAlloc::alloc` and `EspHeap::dealloc` are both
called). Both were checked by disassembly.

**Fix** — `core::hint::black_box` on the probe's pointer, so the allocation
escapes and cannot be removed. The fixed image calls `alloc_caps` and
`dealloc` from the probe. The S3's recorded `largestFreeBlock` moves
214,192 → 213,864 B (`lp-emu:esp32s3:t1`), its first honest value.

**Regression coverage** — the S3 heap ratchet (`just heap-budget-check-chips-s3`)
now pins a `largestFreeBlock` below `freeBytes`, which an elided probe cannot
produce. Nothing asserts the property directly; a disassembly check
(the probe must call the allocator) would.

**Lesson** — an "allocate and free" probe is a measurement only while the
allocation is observable. The optimizer is allowed to prove it away, and
whether it does depends on the global allocator's shape, so the same source
was a measurement on two chips and a constant on the third.

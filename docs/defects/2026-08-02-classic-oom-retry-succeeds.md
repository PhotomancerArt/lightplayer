---
status: open
found: 2026-08-02      # how: hardware-walk
area: fw-esp32v3 (esp-alloc / linked_list_allocator) + lps-glsl typeck
class: allocator-refuses-a-request-it-can-serve
related:
  - 2026-08-01-classic-heap-regression-after-f32-merge.md
---
# The classic OOMs on a 3,072-byte request it satisfies microseconds later

**Symptom** — loading `examples/basic` (241 LEDs, 4,092 B of GLSL) on the
DOM-Z-102 OOMs during on-device shader compilation. With the OOM report's
new instrumentation:

```
allocation failed: requested=3072 align=4 free=3508 used=109128 \
  largest_free=3495 retry_ok=true context=shader node: compile
[OOM] RETRY SUCCEEDED: the same 3072-byte request fits now. The failure was
      not this heap state ...
frames: ...
  <alloc::raw_vec::RawVec<lps_glsl::hir::types::HirExpr>>::grow_one
  <lp_collection::chunked_vec::ChunkedVec<...HirExpr>>::push
  <lps_glsl::hir::typeck::TypeCtx>::type_call
  <lps_glsl::hir::typeck::TypeCtx>::type_expr        (×2)
  <lps_glsl::hir::typeck::TypeCtx>::type_expr_args
  <lps_glsl::hir::typeck::TypeCtx>::type_decl_init
```

Three facts, all from the same handler, all before anything is freed:

1. `free` (3,508) exceeds the request (3,072).
2. `largest_free` (3,495) — binary-searched out of the allocator — also
   exceeds the request, so this is **not** fragmentation.
3. `retry_ok=true`: re-issuing the *identical* `Layout` succeeds.

**Not yet root-caused.** The leading guess is a first-fit edge in
`linked_list_allocator` at the very bottom of the arena — a hole whose
front padding cannot be recorded as a `Hole` (`size_of::<Hole>()` is 8 with
`align_of` 4 on this 32-bit target), which makes the hole unusable for one
layout and usable for another, and which the probe's own allocate/free pair
can normalise away. That is a hypothesis, not a finding: nobody has read the
free list directly.

**Why it is filed rather than fixed** — the board is out of memory in every
practical sense regardless of the edge. `examples/basic` needs 44,488 B of
project resident plus a ~65 KB compile working set against a 112,640 B
arena; the failing allocation is the last 3 KB of a fit that was never going
to be comfortable. The same project fails identically at M3 (`7aff9b10e`),
so no regression is hiding here — that shader has never compiled on the
classic. `projects/test/quad-strips-v3`'s 1,267 B shader still compiles
on-device in 62 ms.

**What would move it** — reading the free list. `linked_list_allocator`
exposes no walk, so this needs either a vendored fork, a switch to
`esp-alloc`'s TLSF algorithm (`ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF`, which
has bounded allocation time and a different hole discipline) for one
measurement, or a reduction in the compile working set so the question stops
mattering on this chip.

**Lesson** — "requested < free" is not evidence of fragmentation, and
`largest_free` is not enough to prove the negative either. Asking the
allocator the caller's own question a second time is a two-line probe that
distinguishes "the heap is in a bad state" from "the allocator is refusing
something it can serve", and those have nothing in common but the error
message. The `retry_ok` line exists because this failure looked, for an
hour, like a heap-capacity problem with a heap-capacity fix.

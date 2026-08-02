---
status: open      # narrowed 2026-08-02; the allocator is exonerated, the window is not
found: 2026-08-02      # how: hardware-walk
area: fw-esp32v3 (esp-alloc / linked_list_allocator) + lps-glsl typeck + lp-collection
class: report-describes-a-later-heap-than-the-failure
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

## Root cause, 2026-08-02

The original hypothesis — a first-fit edge in `linked_list_allocator` that
refuses a hole it could serve, normalised away by the probe's own
allocate/free pair — is **refuted**. So is the framing that fact 1 and 2
were "all from the same handler, before anything is freed": two of the three
are measured after the probe has already had its hands in the free list.

Everything below was measured on the host against `linked_list_allocator`
0.10.5 — the exact version `esp-alloc` 0.10 pins — replaying a 112,640-byte
arena. No hardware, no fork. Harness: see the PR that carries this edit.

### The allocator is exonerated

The refusal edge is real but far too small to be this. `HoleList::split_current`
rejects a hole outright when the leftover would be too small to record as a
`Hole`, so on this 32-bit target (`size_of::<Hole>()` = 8) a hole refuses a
request only when it is **1..8 bytes bigger than the request** — swept
exhaustively: holes of 3,073..3,080 refuse 3,072, and 3,081 upward serve it.
A 3,495-byte hole serves a 3,072-byte request every time.

And `free + used` = 3,508 + 109,128 = **112,636** = `HEAP_SIZE − 4`, i.e. the
whole arena, so there is one region and the accounting is intact. With
`largest_free` (3,495) within 13 bytes of `free` (3,508), essentially all free
memory was **one hole**. One 3.5 KB hole cannot refuse 3,072 bytes.

### The probe does not perturb LLFF — so `retry_ok` is genuine

3,969,868 failing-request states across randomised heaps: **zero** cases where
a request failed and then succeeded after running `largest_free_block()`.
Allocate-then-free is inert for this allocator. `retry_ok=true` was therefore a
true statement about the heap — arrived at through reasoning nobody had
checked, and one that would not survive a switch to `esp-alloc`'s TLSF.

### Therefore: the report describes a heap the failing allocation never saw

If the heap at the instant of failure had looked like the printed numbers,
LLFF would have served the request. It did not. So the heap **changed** between
`alloc` returning null and the handler's first read — and the handler does not
mask interrupts until it is already running, which is the window.

That reframes the defect. It is not "the allocator refused something it could
serve"; it is "the OOM report is taken too late to describe the failure". The
class changed accordingly.

### The instrument was also wrong twice

- **Ordering.** `retry_ok` was computed *after* `largest_free_block()`, so the
  one measurement that had to be taken on the caller's heap had ~17
  allocate/free round trips standing in front of it. Now taken first.
- **`largest_free` is not a bound.** Its bisection assumes "if S fits, so does
  S−4", which the refusal edge above makes false: 186 counterexample size-pairs
  in 409,000 probes. It never over-reports (every value it returns did
  allocate), so it is a floor — now documented as one, and no longer the number
  the OOM report leans on.

The OOM path now calls `free_list_shape()`, which reads the list exactly
instead of guessing: take the smallest block the allocator will hand out until
it refuses, and the returned addresses *are* the free list. Validated on the
host — exact hole counts (1/2/5/17), byte-for-byte heap restoration including
the truncation path, and across 300 randomised heaps its `largest` equals the
exhaustively-probed true largest with **zero** error, where the bisection is
only within 16. `holes=1` vs `holes=40` is the entire difference between
exhaustion and fragmentation, and it is now printed rather than inferred.

### And the allocation should never have been 3,072 bytes

`ChunkedVec` exists to keep the compiler off large contiguous allocations. Its
bound was `CHUNK_SIZE = 64` **elements** — not a bound on anything the heap
cares about. Measured with `-Zprint-type-sizes` on `riscv32imac` (same 32-bit
layout as Xtensa):

| element | size | chunk under the old bound |
|---|---|---|
| `LpirOp` | 20 B | 1,280 B — what the "~2–4 KB" comment was calibrated against |
| `HirExpr` | **96 B** | **6,144 B** |

`lps-glsl` reused the collection for `HirExpr` and the bound silently became
6 KB. The last chunk grows by doubling (`MIN_NON_ZERO_CAP` = 4 for a 96-byte
element), so a chunk's allocations run 384 → 768 → 1,536 → **3,072** → 6,144 B
— the failing request is exactly the 32-element step, and the next one would
have asked for 6,144 B with 3,072 B still live beside it for the copy. The
collection whose job was to avoid the largest allocation in the compiler was
making it.

`CHUNK_BYTES = 1024` replaces it, with the element count derived per type
(`HirExpr` → 10 per chunk, 960 B). The 3,072-byte request no longer exists.

## Status

- **Fixed:** the `ChunkedVec` element-vs-byte bound; the probe ordering; the
  `largest_free` overclaim; the missing free-list read.
- **Open:** what freed memory between the null return and the handler. The
  corrected report answers it on the next failure — if `holes` is 1 and
  `retry_ok` is still true, the window is real and the next step is capturing
  free/used at the failure site (esp-alloc's `alloc-hooks` feature fires a hook
  with a null pointer at exactly that moment) rather than in the handler.
- **Unverified on silicon.** Everything above is host measurement and code
  reading. Whether `examples/basic` now compiles on the DOM-Z-102 is a hardware
  question: it needs 44,488 B of project resident plus a compile working set
  against a 112,640 B arena, and removing a 3 KB request does not by itself
  make that fit. The same project fails identically at M3 (`7aff9b10e`), so no
  regression is hiding here — that shader has never compiled on the classic.
  `projects/test/quad-strips-v3`'s 1,267 B shader still compiles in 62 ms.

**Lesson** — "requested < free" is not evidence of fragmentation, `largest_free`
is not enough to prove the negative either, and a probe that changes the thing
it measures cannot be the last thing you run before the measurement that
matters. Asking the allocator the caller's own question a second time is a
two-line probe worth having — but only *first*, and its answer means "the heap
changed", not "the allocator is broken". The `retry_ok` line survived only
because a replay on the host later proved the probe inert; that proof should
not have been load-bearing after the fact.

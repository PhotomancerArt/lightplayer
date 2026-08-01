# Defect: the ABI's withheld sret register was allocated anyway (Xtensa)

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `lpvm-native` register pool (`regalloc/pool.rs`) — **shared**, not ISA-specific
- **Found by:** the Xtensa filetest corpus

## Symptom

**233 load/store traps** (`EXC_LOAD_STORE_ERROR`, cause 3) across 76 corpus
files, overwhelmingly `matrix/` — every aggregate-returning shader. The faulting
address was not garbage but a *shader value*: e.g.
`Trap { cause: 3, vaddr: 229376 }`, and `229376 = 0x38000 = 3.5` in Q16.16 — one
of the matrix elements the test constructs.

A data value was being used as a pointer.

## Cause

A function returning more than two scalars returns through an **sret buffer**
whose pointer arrives in `a2` and must stay there for the entire function.
`isa/xt/abi.rs::func_abi_xt` encodes exactly that:

```rust
let mut allocatable = alloca_base_int();
if is_sret {
    allocatable.remove(A2);   // the sret pointer lives in a2 for the whole function
}
```

**That exclusion was computed and then ignored.** `RegPool::new(isa)` seeded
itself from `isa.allocatable_pool_order()` — the ISA's *static* pool — and never
consulted `FuncAbi::allocatable`. So `a2` stayed available, the allocator handed
it to an ordinary value, and the sret pointer was destroyed:

```
entry a1, 272
or    a2, a3, a3      ; vmctx -> a2, overwriting the sret pointer
...
s32i  …, a2, …        ; stores the result through a matrix element
```

## Why rv32 never saw it

Again register-layout luck. rv32's sret pointer is preserved in `s1` (x9), and
rv32's allocatable pool is `[29,30,31,18..27]` — **`s1` is not in it**. The
`allocatable.remove(S1)` call is a no-op there, so ignoring the ABI's set costs
nothing. `func_abi.rs` even has a test asserting the exclusion
(`sret_excludes_s1_from_allocatable`) — it verified the ABI's *opinion*, which
nothing downstream honoured.

On Xtensa `a2` is squarely inside the pool, so the exclusion is load-bearing.

## Fix

`RegPool` now stores its **effective** pool order rather than re-reading the
ISA's static list, and `RegPool::for_abi(func_abi)` seeds it from the function's
allocatable set. Storing it matters as much as filtering it: `clear` and
`clear_all` reseed the LRU, and reseeding from the static list would have handed
the withheld register straight back mid-function.

## Regression test

`regalloc::pool::tests::for_abi_withholds_the_sret_pointer_register` — builds a
vec4-returning `FuncAbi`, asserts the ABI withholds `a2` (the precondition), then
allocates past pool capacity *and across a `clear`*, asserting `a2` is never
handed out. Verified to fail when the fix is reverted.

## Impact

Xtensa corpus: 6072 → 6303 passing cases, 730 → 805 passing files. Every
cause-3 trap disappeared.

## Lesson

An ABI that *computes* a constraint proves nothing; something has to *enforce*
it. The `sret_excludes_s1_from_allocatable` unit test passed for the entire life
of the bug because it tested the producer, never the consumer. When a value
object exists to constrain another component, the test worth having is the one
that asserts the constraint survives the hand-off.

Companion defect from the same session and the same class:
`2026-07-30-xtensa-call-argument-clobber.md`.

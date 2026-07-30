# Defect: call-argument staging was not a parallel move (Xtensa)

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `lpvm-native` register allocation (`regalloc/walk.rs`) — **shared**, not ISA-specific
- **Found by:** the Xtensa filetest corpus, on its first run

## Symptom

Xtensa function calls silently passed **duplicated** argument values. No trap,
no crash — the callee computed on a plausible-looking wrong input. On the corpus
this appeared as ~100 wrong-value failures across `function/`, `vec/`, `lpvm/`
and `array/`, e.g. `array/phase/7-function-parameters.glsl:141` returning 110
where 185 was expected.

Minimal repro (`f()` calls `g(1, 2, 4)` and `g` sums its parameters):

```
Movi(a14, 1)       ; p0
Movi(a13, 2)       ; p1
Movi(a12, 4)       ; p2
Or(a10, a15, a15)  ; a10 <- vmctx   ✓
Or(a11, a14, a14)  ; a11 <- 1       ✓
Or(a12, a13, a13)  ; a12 <- 2       ✓ …and clobbers a12, which still held p2 = 4
Or(a13, a12, a12)  ; a13 <- 2       ✗ should be 4
```

Result `0 + 1 + 2 + 2 = 5`, expected `7`.

## Cause

The ABI requires argument staging to behave as if all moves happened
**simultaneously** — every argument reaches its register carrying the value it
had before any move ran. `regalloc/walk.rs` emitted them sequentially in
argument order, with no check that a move's destination was still another
move's source. Classic parallel-move problem.

Two sites had it: the caller's argument staging (Phase B of `process_call`) and
the callee's entry parameter moves (`finish`), the latter also having an
ordering dependency with incoming-stack-argument loads.

## Why rv32 never saw it

This is **shared code**, and rv32 exercises it constantly. It is safe there only
because of register-layout luck: rv32's argument registers are `a0..a7` (hw
10..17) and its allocatable pool is 18..31 — **disjoint**, so a pool home can
never be an argument register, so no destination is ever another move's source.

Xtensa's caller-view staging bank `a10..a15` **is** the caller-saved half of its
own 12-register allocatable pool. The hazard is not occasional there; it fires as
soon as three arguments are in play.

## Why the existing tests missed it

M3's `cross_function_call_via_literal_slot_callx8` passes exactly **one**
argument. The defect needs three. Nothing else in `xt_pipeline` made a
multi-argument guest→guest call.

## Fix

`regalloc/walk::sequence_arg_moves` orders the moves so none clobbers a live
source, breaking cycles through a scratch register obtained from the new
`IsaTarget::move_cycle_scratch_hw` hook (rv32 `t3`, Xtensa `a9` — both outside
the allocatable pool). Applied at both sites; the entry path additionally splits
its edits into four dependency-ordered groups.

On rv32 the transform is an identity (no destination is ever a source), and the
rv32 corpus is byte-identical across the change.

## Regression test

`lpvm-native/tests/xt_pipeline.rs::multi_arg_call_passes_every_argument`, one
case per arity 1..=20 (1..=11 for this defect), arguments being distinct powers
of two so any dropped, duplicated or misplaced argument changes the sum.

## Closed gap (was: 12 user arguments still fails)

This entry originally left 12 arguments open — expected 4095, got 2063 — and
guessed the cause was spilling around the call. It was not spilling: the fix
here made the *register* half of the argument transfer a parallel move and left
the *stack* half out of the graph, and the emitter stores the stack half after
every one of these moves has run. Diagnosed and fixed the same day in
`2026-07-30-xtensa-stack-arg-staged-over.md`, which also lifted seven
`vec/*/from-scalar.glsl` files.

One file named here was misattributed:
`lpvm/native/perf/call-clobber-correctness.glsl` scored 6/7 both before and
after that fix. Its remaining directive is a return-path failure, not an
argument-path one — see the companion entry.

## Lesson

Two defects found the same day (see
`2026-07-30-xtensa-sret-pointer-clobber.md`) were both **shared allocator code
that rv32's register layout made unfalsifiable**. rv32 passing is not evidence
that shared code is correct — it is evidence that rv32's layout avoids the case.
A second ISA with an overlapping pool is what turns those into observable bugs,
which is precisely what the Xtensa corpus was landed to do.

Corollary for repros: a hand-built LPIR module that fails on *both* ISAs is an
invalid module, not a finding. Run every hand-built repro on rv32 first — an
earlier version of this investigation omitted `VMCTX_VREG` from the call and
briefly read as "no bug here".

# Defect: stack-passed call arguments were left out of the parallel move (Xtensa)

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `lpvm-native` register allocation (`regalloc/walk.rs`) — **shared**, not ISA-specific
- **Found by:** the Xtensa filetest corpus — the known gap left open by
  `2026-07-30-xtensa-call-argument-clobber.md`

## Symptom

A guest→guest call with **12 or more** arguments passed the wrong value for
whichever argument overflowed to the stack. Arities 1..=11 were correct. No
trap: the callee summed a plausible-looking wrong input.

`xt_pipeline::multi_arg_call_passes_every_argument` at arity 12 expected 4095
and got 2063 — off by `2048 - 16`, i.e. argument 11 (2048) arrived carrying
argument 4's value (16).

Corpus face: `function/param-many.glsl`, `function/param-mixed.glsl`,
`lpvm/native/perf/stack-args-outgoing.glsl`,
`lpvm/native/spill_pressure_call.glsl`, and all seven
`vec/{vec3,vec4,ivec3,ivec4,uvec3,uvec4,bvec4}/from-scalar.glsl`
(`*_as_first_call_arg` directives — same mechanism, confirmed by the fix).

## Cause

Xtensa passes six arguments in registers and the rest in the caller's outgoing
stack area. The two halves are emitted by **different components at different
times**:

- the register half by `regalloc/walk.rs`, as `Before(call)` edits;
- the stack half by the ISA emitter, inside `isa/xt/emit.rs::emit_call` — which
  runs *after* every one of those edits.

`sequence_arg_moves` (the fix for the companion defect) made the register half a
correct parallel move. But it only ever saw the register half. A stack-passed
argument's home register is live across all those staging moves, and Phase B1
recorded that register as the emitter's source without checking whether a
staging move overwrites it first.

The trace shows it in three instructions. `a15` holds argument 11 (`0x800`), and
`a15` is also the staging register for argument 4:

```
l32r  a15, …          ; a15 <- 0x00000800   argument 11
or    a15, a10, a10   ; a15 <- 0x00000010   argument 4 staged over it
…
s32i  a15, a1, 24     ; mem[…] <- 0x00000010   ✗ should be 0x800
```

Twelve is exactly where it starts: twelve live values fill Xtensa's
12-register pool, so the overflow argument has nowhere to sit except a
staging register.

## Why rv32 never saw it

The same reason as its two companions. rv32's staging targets are the argument
registers (hw 10..17) and a pool home comes from 18..31 — **disjoint**, so a
stack-passed argument's home is never a staging destination and the store always
reads the right register. On Xtensa the staging bank `a10..a15` *is* the
caller-saved half of the pool.

`sequence_arg_moves` had already established that the argument transfer is a
parallel move. What it got wrong is the **extent** of that move: the moves and
the outgoing stores are one simultaneous transfer, and only the moves were in
the graph.

## Fix

Phase B and Phase B1 of `process_call` swap order, so the stack-argument
decision can see the staging moves. Where a stack-passed argument's home is a
register the staging moves write, the value is parked in its spill slot by a
`Before(call)` edit and the emitter is handed `Alloc::Stack(slot)` instead —
both emitters already reload an outgoing `Stack` argument through their scratch
register.

The park edits sit **after** the Phase-A reloads and **before** the staging
moves. That is the only correct position: a stack-passed argument may itself
have just been reloaded into the home being parked.

On rv32 the branch is unreachable (no home is ever a staging target), and the
rv32 corpus is byte-identical across the change.

## Regression coverage

`lpvm-native/tests/xt_pipeline.rs::multi_arg_call_passes_every_argument`,
extended from arities 1..=11 to 1..=20. Negative-controlled: with the fix
reverted, cases 12..=20 fail and 1..=11 still pass — the arity-12 boundary the
mechanism predicts.

## Impact

| target | cases | files |
| --- | --- | --- |
| `xtn.q32` | 6303 → **6316** | 805 → **816** |
| `xtlpn.q32` | 6356 → **6370** | 812 → **823** |
| `rv32*` / `wasm` | 31587 → 31587 | 851 → 851 |

No regressions on any target; filetest baselines byte-identical.

## Still open, and *not* this mechanism

`lpvm/native/perf/call-clobber-correctness.glsl` was listed as a face of this
defect in the companion entry. It is not: the file scored **6/7 both before and
after** this fix. Its one failing directive is `test_interleaved_vec2()` —
`vec2(4.0, 6.0)` expected, `vec2(4.0, 5.0)` observed — where the *first* of two
two-value-returning calls must survive the second. That is the return path, not
the argument path. A hand-built LPIR module of that shape (two 2-return calls,
first result live across the second) passes on Xtensa, so the simple
return-register hazard is not it and it remains undiagnosed. Not expanded here
on purpose.

## Lesson

The companion fix was right about the mechanism and wrong about its boundary. It
modelled "the argument staging moves" as the thing that must be simultaneous,
when the ABI-level unit is "the argument transfer" — every write that must
appear to happen at once, wherever it is emitted from and whichever component
emits it. Half of that transfer lived in another crate's emitter and was
invisible to the graph.

So when a fix introduces a simultaneity constraint, the question worth asking is
not "did I order these operations correctly" but "what is the complete set of
operations this constraint covers, and does anything in it get emitted somewhere
I am not looking." Here the answer was one loop in `isa/*/emit.rs`, and the gap
stayed open for exactly as long as it took to ask.

Companion defects, same day and same class:
`2026-07-30-xtensa-call-argument-clobber.md`,
`2026-07-30-xtensa-sret-pointer-clobber.md`.

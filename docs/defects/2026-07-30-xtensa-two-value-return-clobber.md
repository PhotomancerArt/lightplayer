# Defect: two-value returns were not a parallel move (Xtensa)

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `lpvm-native` register allocation (`regalloc/walk.rs`) — **shared**, not ISA-specific
- **Found by:** the Xtensa filetest corpus — the last un-triaged failure in
  `lpvm/native/perf/call-clobber-correctness.glsl`

## Symptom

A function returning two scalars — a `vec2` — silently lost its **second**
component when the result had to survive a later call. The component was
replaced by the *first* component, not by garbage.

```glsl
vec2 make_vec2(float x, float y) { return vec2(x, y); }

vec2 t_first_survives() {
    vec2 a = make_vec2(1.0, 2.0);
    vec2 b = make_vec2(3.0, 4.0);
    return a;                       // vec2(1.0, 1.0)  — expected vec2(1.0, 2.0)
}
```

Corpus face: `call-clobber-correctness.glsl:99`,
`test_interleaved_vec2() ~= vec2(4.0, 6.0)` returning `vec2(4.0, 5.0)` — the
`5.0` being `1.0 + 4.0` where `2.0 + 4.0` was meant.

## Cause

A direct two-value return arrives in the caller-view return bank `a10`/`a11`.
`process_call` moved each return value to its pool home **in return order**,
with no check that one move's destination was still another's source:

```
callx8 a8          ; make_vec2 returns (a10, a11) = (1.0, 2.0)
or a11, a10, a10   ; ret0 -> pool home a11 … clobbering a11, which holds ret1
or a12, a11, a11   ; ret1 -> pool home a12 … reading the clobbered value
```

The allocator's own log shows it directly:

```
; move: Reg(a10) -> Reg(a11)
; move: Reg(a11) -> Reg(a12)
```

This is the **same parallel-move defect as its three companions, in the return
direction**. `sequence_arg_moves` — written for the argument direction and
correct — was simply never called on the return path.

## Why rv32 never saw it

The same register-layout luck, a fourth time. rv32's return registers are
`a0`/`a1` (hw 10..11) and its allocatable pool is 18..31 — **disjoint**, so a
pool home can never be a return register and no destination is ever a source.
On Xtensa the caller-view return bank `a10`/`a11` sits inside its own
12-register pool.

## Why it was the last one standing

Two things hid it after the argument-side fixes landed:

1. **It needs `vec2`, not `vec3`.** Three or more scalars return through an
   sret buffer, which is loaded component-by-component from memory and has no
   parallel-move hazard at all. Only the 2-value *direct* return path is
   affected — verified: the `vec3` form of the same shader passes.
2. **Hand-built LPIR repros of the shape kept passing.** Three separate attempts
   (including summing the second components of two calls) all had the allocator
   spill the returns to slots instead of assigning the colliding home, so the
   hazard never arose. The defect needs the allocator to place ret0's home
   *exactly* on `a11`, which the GLSL pipeline's register pressure produced and
   hand-built IR did not.

That second point is the reusable lesson — see below.

## Fix

`process_call` Step 1 now collects the return moves and runs them through the
same `sequence_arg_moves` used for arguments. The After(call) return group is
ordered:

1. `Reg(ret_reg) -> Stack(slot)` stores — they read a return register, so they
   precede any move that writes one (a store touches no register, so it cannot
   disturb the moves);
2. the sequenced reg→reg moves;
3. `Reg(pool_home) -> Stack(slot)` write-throughs — they read a move
   *destination*, so they follow.

On rv32 the transform is an identity and the corpus is byte-identical.

## Regression coverage

`lpvm/native/perf/call-clobber-correctness.glsl:99` — **6/7 → 7/7** across the
fix, negative-controlled.

`regalloc::walk::tests::sequence_arg_moves_orders_return_values_out_of_the_return_bank`
pins the move set, but is explicitly a **characterization** test, not the
regression: the sequencer was always correct, so reverting the fix does not fail
it. Its value is that a future refactor of the sequencer cannot break the return
direction silently.

## Impact

| target | cases | files |
| --- | --- | --- |
| `xtn.q32` | 6316 → **6317** | 816 → **817** |
| `xtlpn.q32` | 6370 → **6371** | 823 → **824** |
| `rv32*` / `wasm` | 31587 → 31587 | 851 → 851 |

One case — but it was the last *unexplained* wrong-value failure on Xtensa.

## Lesson

Three fixes had established that call-argument staging is a parallel move. None
asked the obvious dual question: **returns are a transfer through the same
overlapping register bank, in the opposite direction.** The abstraction was
right and its application was half-scoped, twice over — first missing the
outgoing stack arguments, then missing the return path entirely.

When a fix introduces an invariant, the cheap next move is to enumerate every
place the same invariant applies rather than only the site that failed. Here
that enumeration is one sentence long — *argument registers in, return
registers out* — and it would have found this on 2026-07-30 morning instead of
evening.

Second, on repro method: a hand-built IR repro that **passes** is weak evidence.
Three attempts here passed because the allocator made a different, benign choice
each time; the defect required an exact register assignment that only the real
GLSL pipeline's pressure produced. When a minimal repro won't reproduce, shrink
the *real* input instead of synthesizing a smaller one — the GLSL minimization
took four variants and isolated it immediately, including the `vec2`-vs-`vec3`
boundary that named the mechanism.

Companion defects, same day and same class:
`2026-07-30-xtensa-call-argument-clobber.md`,
`2026-07-30-xtensa-sret-pointer-clobber.md`,
`2026-07-30-xtensa-stack-arg-staged-over.md`.

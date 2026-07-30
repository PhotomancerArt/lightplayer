---
status: fixed
found: 2026-07-30      # how: ci (Xtensa filetest corpus triage)
fixed: this change
area: lpvm-native lowering (`lower.rs`) — **shared**, not ISA-specific
class: config-masked-defect
related:
  - docs/adr/2026-07-30-integer-division-never-traps.md
  - docs/defects/2026-07-30-xtensa-call-argument-clobber.md
  - docs/defects/2026-07-30-xtensa-sret-pointer-clobber.md
  - docs/defects/2026-07-30-xtensa-stack-arg-staged-over.md
  - docs/defects/2026-07-30-xtensa-two-value-return-clobber.md
---
# Defect: integer divide by zero took the device down on Xtensa

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `lpvm-native` lowering (`lower.rs`) — **shared**, not ISA-specific
- **Found by:** the Xtensa filetest corpus

## Symptom

**23 cases across 3 corpus files** failing on both Xtensa targets with
`Trap { kind: Exception, cause: 6 }` (`EXCCAUSE 6`, `IntegerDivideByZero`):

```
scalar/int/op-divide-by-zero.glsl    8/16 passing
scalar/int/op-modulo-by-zero.glsl    6/12 passing
scalar/uint/op-divide-by-zero.glsl   2/11 passing
```

Not a wrong value — a **trap**. On silicon that is a shader dividing by zero
taking the board down, and the GLSL that triggers it is the ordinary guarded
idiom, which the corpus pins directly:

```glsl
// i == 0; eager && still runs the divide, which must produce -1, not trap.
if (i != 0 && (x / i > 0)) { ... }
```

## Cause

`lower.rs` lowered all four LPIR integer divide/remainder ops to a bare
`VInst::AluRRR`, and `isa/xt/emit.rs` maps those straight to `QUOS` / `QUOU` /
`REMS` / `REMU`. Those instructions raise `EXCCAUSE 6` on a zero divisor.

The LPIR contract (`docs/design/lpir/02-core-ops.md`) has always said integer
division and remainder **never trap** and follow RV32M semantics — `x / 0` is
all ones, `x % 0` is the dividend. RV32M gives those results in hardware, so
the bare instruction *is* the correct lowering on rv32 and needed no guard.
`lpvm-wasm` and `lpvm-cranelift` each emit their own guard because their native
instructions trap.

The shared lowering therefore encoded an rv32 hardware property as if it were a
property of all targets. Xtensa arrived and the assumption became a trap.

## Why nobody caught it earlier

Two documents pointed the wrong way at once:

- The guard obligation named a **closed list** of backends —
  "(WebAssembly, Cranelift)" — rather than stating a rule. A new backend author
  reading it had no reason to think it applied to them.
- `docs/design/lpir/00-overview.md`'s summary table — the row a backend author
  reads *first* — said integer division by zero yields `0`. That is wrong for
  both operations and disagrees with every implemented backend.

So the contract was correct in one place, contradicted in another, and its
obligation was phrased as a roster instead of a rule.

## Fix

- `IsaTarget::integer_div_traps_on_zero()` (`isa/mod.rs`) names the property
  instead of assuming it: `false` for rv32, `true` for Xtensa.
- `lower.rs` gains `lower_int_div` / `emit_guarded_int_div`. When the hook says
  the ISA traps, the divide is guarded at the **VInst** level so register
  allocation owns the temporaries — not in the emitter with hand-managed
  scratch registers, which is the shape that produced the four allocator
  entries alongside this one. rv32 still emits the bare instruction, byte for byte.
- Only the **zero divisor** is guarded on Xtensa. Its divide already yields
  `i32::MIN` for `i32::MIN / -1` and `0` for `i32::MIN % -1`, matching RV32M, so
  guarding that column too would be wasted instructions. Cranelift guards both
  because its `sdiv` traps on both. The guard set is per-ISA on purpose.
- Docs: the overview row corrected; the obligation restated as a rule that
  applies to any diverging backend, with the current backends as examples.

Cost, measured: 9 Xtensa instructions (27 B) for a divide and 10 (30 B) for a
remainder, replacing one 3-byte instruction. Full reasoning and alternatives in
`docs/adr/2026-07-30-integer-division-never-traps.md`.

## Regression coverage

`lp-shader/lpvm-native/tests/xt_pipeline.rs` — `idiv_s_by_zero_is_all_ones_not_a_trap`,
`idiv_u_by_zero_is_all_ones_not_a_trap`, `irem_s_by_zero_is_the_dividend_not_a_trap`,
`irem_u_by_zero_is_the_dividend_not_a_trap`,
`div_guard_is_correct_when_dst_aliases_an_operand`,
`guarded_division_idiom_does_not_trap_when_the_guard_is_false`, plus two
controls — `int_min_over_minus_one_needs_no_guard_on_xtensa` and
`nonzero_divisors_are_unperturbed_by_the_guard` — which pin the cases the guard
must *not* touch. All six negative-controlled: with the hook forced to `false`
they fail as `cause: 6` traps while both controls keep passing.

The corpus files above are the end-to-end coverage: both Xtensa targets now
reach 39/39 directives and 3/3 files.

## Lesson

This is the **fifth `config-masked-defect` on 2026-07-30**, all in
`lpvm-native`, all the same shape: shared code that was correct only because
rv32 happened to make it correct. The other four were register-layout
coincidences in `regalloc/`; this one is a hardware-semantics coincidence in
lowering, which is the same mechanism one level down.

What distinguishes it is worth keeping: **the falsifying test already existed.**
`op-divide-by-zero.glsl` has pinned this contract for as long as the corpus has
existed — it simply had no Xtensa target to run against. The gap was not
missing coverage but a missing *backend*, plus documentation that told the
backend author the obligation was somebody else's.

The generalizable rule: when a contract is satisfied *for free* on the
reference target, that is exactly when it needs to be stated as an obligation
with a named capability hook, because nothing in the code will ever remind you
it is a choice. A closed list of affected backends is a bug report waiting for
its next entry; write the rule and let each ISA answer it.

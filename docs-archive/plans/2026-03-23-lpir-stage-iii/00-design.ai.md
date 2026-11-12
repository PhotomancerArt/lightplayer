# LPIR Stage III — Design

## Scope of work

Harden the `lpir` crate with comprehensive interpreter and validator test
coverage. No new features — only tests and bug fixes discovered during testing.

Spec: `docs/lpir/` (chapters 00–09).
Roadmap: `docs/roadmaps/2026-03-21-lpir/stage-iii.md`.

## File structure

```
lp-shader/lpir/src/
├── tests.rs                           # UPDATE: keep round-trip, sizing, smoke tests;
│                                      #   remove validator negatives (moved)
├── tests/
│   ├── all_ops_roundtrip.rs           # EXISTING: unchanged
│   ├── interp.rs                      # NEW: comprehensive interpreter tests
│   └── validate.rs                    # NEW: validator positive/negative tests
├── interp.rs                          # UPDATE: bug fixes only (if discovered)
└── validate.rs                        # UPDATE: bug fixes only (if discovered)
```

## Architecture

```
tests/interp.rs
├── Helpers
│   ├── run(ir, func, args) → Vec<Value>         reusable parse+interpret
│   ├── run_i32(ir, func, args) → i32             shorthand
│   ├── run_f32(ir, func, args) → f32             shorthand
│   ├── NoImports                                  stub handler
│   └── MockMathImports                            mock for @std.math::*
│
├── Float arithmetic          fadd, fsub, fmul, fdiv, fneg
├── Integer arithmetic        iadd, isub, imul, idiv_s, idiv_u, irem_s, irem_u, ineg
├── Float comparisons         feq, fne, flt, fle, fgt, fge (incl. NaN)
├── Integer comparisons       ieq, ine, ilt_s/u, ile_s/u, igt_s/u, ige_s/u
├── Logic / bitwise           iand, ior, ixor, ibnot, ishl, ishr_s, ishr_u
├── Constants                 fconst.f32, iconst.i32
├── Immediate variants        iadd_imm, isub_imm, imul_imm, ishl_imm, ishr_s/u_imm, ieq_imm
├── Casts                     ftoi_sat_s, ftoi_sat_u, itof_s, itof_u
├── Select / copy             select, copy
├── Edge-case numerics        div-by-zero→0, rem-by-zero→0, NaN propagation,
│                              saturating casts (overflow, NaN, neg), shift masking (≥32)
├── Control flow              if, if/else, loop+br_if_not, loop+break, loop+continue,
│                              nested loops, switch (match, default, no-match), early return
├── Memory                    slot_addr, load, store, memcpy, dynamic index
├── Calls                     local call, import call (mock), multi-return, recursion
├── Stack overflow            unbounded recursion → InterpError::StackOverflow
└── Error paths               function not found, arg arity mismatch

tests/validate.rs
├── Positive                  valid modules pass (spec examples)
├── VReg errors               undefined, out-of-range
├── Control flow errors       break/continue/br_if_not outside loop, else without if
├── Call errors               callee OOB, arg arity, arg type, result type
├── Return errors             arity mismatch, type mismatch
├── Module errors             duplicate entry, duplicate func name, duplicate import
├── Memory errors             slot_addr OOB, pool OOB
├── Switch errors             duplicate case, duplicate default
└── Type errors               copy type mismatch, select type mismatch, opcode dst type
```

## Phases

```
1. Reorganize test files
2. Interpreter tests: arithmetic, comparisons, logic, constants, immediates, casts, select/copy
3. Interpreter tests: edge-case numerics
4. Interpreter tests: control flow, memory, calls, stack overflow, error paths
5. Cleanup & validation
```

# Design — LPIR parity stage I (relational expressions)

## Scope of work

Implement Milestone I from
[
`docs/roadmaps/2026-03-29-lpir-parity/milestone-i-relational-expressions.md`](../../roadmaps-old/2026-03-29-lpir-parity/milestone-i-relational-expressions.md):

- Correct **type shape** for `Expression::Relational` in `expr_scalar.rs`.
- Align **`lower_relational`** with **[`docs/design/q32.md`](../../design/q32.md) §6** (`isnan` /
  `isinf` always false on Q32).
- Fix **filetest sources** that Naga rejects (`common-isnan.glsl`, `common-isinf.glsl`).
- **Unmark** and verify **bvec relational** (`vec/bvec{2,3,4}/`, plus relational-only cases
  elsewhere), **matrix equality**, and those builtins.
- **Parity bar:** Tier A (and Tier B as listed in [`summary.md`](./summary.md)) must pass on
  **`jit.q32`**, **`wasm.q32`**, and **`rv32.q32`**. See
  [`expected-passing-tests.md`](./expected-passing-tests.md).

**Normative Q32:** [`docs/design/q32.md`](../../design/q32.md).

## File structure

```
lp-shader/lps-naga/src/
├── expr_scalar.rs              # UPDATE: expr_type_inner + expr_scalar_kind for Relational
├── lower_expr.rs               # UPDATE: lower_relational, isnan/isinf → lane false (Q32)
└── ...

lp-shader/lps-filetests/filetests/
├── builtins/common-isnan.glsl  # UPDATE: no infinite literals
├── builtins/common-isinf.glsl # UPDATE: no infinite literals
├── vec/bvec{2,3,4}/…            # UPDATE: strip @unimplemented where fixed (relational-only)
└── matrix/mat{2,3,4}/op-{equal,not-equal}.glsl  # same

docs/plans/2026-03-29-lpir-parity-stage-i/
├── summary.md                  # NEW (end): what shipped
└── ...
```

## Conceptual architecture

```
Naga Expression::Relational { fun, argument }
        │
        ├── expr_type_inner / expr_scalar_kind  (compile-time shape for lowering & stores)
        │        • All / Any     → result: scalar bool
        │        • IsNan / IsInf → result: bvecN (same width as float vec argument)
        │        • Not (if present) → result: bvecN (same as argument)
        │
        └── lower_relational → VRegVec
                 • All / Any     → Iand / Ior chain → one I32 lane
                 • IsNan / IsInf → per lane: IconstI32(0)  [Q32 per q32.md §6]
                 • Not           → per lane: Ieq(lane, 0)   [if Naga uses Relational::Not]
```

Matrix `m1 == m2`:

- Naga: component-wise `==` → **vector of bool** → **`all(...)`** → scalar bool.
- Lowering: existing `lower_binary_vec` + `Feq` lanes; **`all`** needs working `expr_type_inner` for
  `Relational` so parent expressions type-check in the walker.

## Main components and how they interact

| Component          | Role                                                                                                                                                                      |
|--------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `expr_type_inner`  | Supplies `TypeInner` for every Naga expression handle; must not return `Err` on `Relational` when GLSL is valid.                                                          |
| `expr_scalar_kind` | Used where a single **scalar** kind is required; for `All`/`Any` result use **`Bool`** scalar, not the bvec’s “vector of bool” kind shortcut.                             |
| `lower_relational` | Emits LPIR bool lanes (`I32`); Q32 `isnan`/`isinf` must not use IEEE tricks or div0 sentinel tests.                                                                       |
| Filetests          | Prove **jit + wasm + rv32** pass for the [expected corpus](./expected-passing-tests.md); strip `@unimplemented(backend=…)` that blocked any backend once LPIR is correct. |

**Note:** `lps-naga` lowering is **float-mode agnostic** at the IR level; all current filetest
targets are **q32** (`jit.q32`, `wasm.q32`, `rv32.q32`). Emitting constant-false `isnan`/`isinf`
matches [`q32.md`](../../design/q32.md) for those targets. A future **jit.f32** target would need a
separate policy or `LowerCtx` numeric mode (out of scope unless introduced).

## Related notes

Full Q&A and codebase snapshot: [`00-notes.md`](./00-notes.md).

Explicit pass list + three-target rules: [`expected-passing-tests.md`](./expected-passing-tests.md).

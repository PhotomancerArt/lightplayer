# Design: LPIR parity — stage II (Milestone II — pointer / Access lowering)

## Scope of work

Close gaps between **Naga’s** `Expression::Access` / compound-update **store** shapes and *
*`lps-frontend`** lowering so Milestone II filetests pass on **`jit.q32`**, **`wasm.q32`**, and *
*`rv32.q32`** (see [`expected-passing-tests.md`](./expected-passing-tests.md)). In scope:

- **Loads** through **`Access`** on local vectors (and matching **Load** peel rules).
- **Stores** through **`Access`** (e.g. `bvec[0] =`, matrix element / column patterns Naga emits for
  `++m[i][j]`).
- **Matrix column** vector load/store and **column / element** inc-dec where Naga matches roadmap
  tests.

Out of scope: general **arrays** (Milestone IV), **structs**, matrix **invoke** ABI (Milestone V).

## File structure

```
lp-shader/lps-frontend/src/
├── lower_expr.rs              # UPDATE: Expression::Access, Load { pointer: Access… }
├── lower_stmt.rs              # UPDATE: Statement::Store through Access / matrix column
├── lower_access.rs            # NEW (optional): shared Access index → vreg helpers
├── expr_scalar.rs             # UPDATE if Access types need expr_type_inner fixes
└── lower_ctx.rs               # TOUCH only if new helpers need layout helpers

lp-shader/lps-filetests/filetests/
├── matrix/**/incdec-matrix-*.glsl
├── operators/incdec-matrix*.glsl
└── vec/bvec*/assign-element.glsl, index-variable*.glsl, access-array.glsl

docs/plans/2026-03-29-lpir-parity-stage-ii/
├── 00-notes.md
├── 00-design.md
├── expected-passing-tests.md
├── 01-phase-naga-shapes-and-access-load.md
├── 02-phase-store-access-vector-matrix.md
├── 03-phase-matrix-column-and-compound.md
├── 04-phase-filetests-three-targets.md
└── 05-phase-cleanup-validation.md
```

If `lower_access.rs` would be tiny, keep helpers at the **bottom** of `lower_expr.rs` /
`lower_stmt.rs` instead; split only when clarity wins.

## Conceptual architecture

```
GLSL  →  Naga IR  →  lps-frontend
              │              │
              │    Expression::Access { base, index }
              │              │
              ├──────────────┼──► lower_expr: index vreg + select chain
              │              │         → scalar or column VRegVec
              │              │
              │    Store(pointer=Access…, value)
              │              │
              └──────────────┼──► lower_stmt: resolve base vregs + mask write
                             │         (per-lane Copy or column slice)
                             │
              Existing: AccessIndex(Local), AccessIndex(AccessIndex(Local))
                             │
                             └──► unchanged where Naga still emits them
```

**Vector / bvec dynamic read:** Evaluate **base** to `VRegVec`, **index** to one **I32** vreg; for
size `N ∈ {2,3,4}`, emit **`ieq`** against `0..N-1` and nest **`select`** (default last lane) to one
scalar vreg. Optionally align with GLSL bounds semantics (see phase notes on **EXPECT_TRAP** tests).

**Vector / bvec indexed store:** Load current lanes (or keep in hand), replace lane `k` with coerced
scalar, write back with **`Copy`** to each component vreg (no memory unless we add stack path
later).

**Matrix:** Map **`Access`** to **column** (first index) or **element** (second index) using same *
*column-major flat** layout as `lower_stmt` nested `AccessIndex` (`col * rows + row`). Compound
updates (`++`) are expected to lower to **load → arithmetic → store** through the same pointer
shape; implementation extends **Store** / **Load** matching, not the GLSL front-end.

## Main components and how they interact

| Component         | Role                                                                                                                                                             |
|-------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `expr_type_inner` | Must return correct types for `Access` results (scalar column, matrix element) so `coerce_assignment_vregs` and stores type-check.                               |
| `lower_expr`      | Implements **`Access`** for vectors and matrices; extends **`Load`** to recurse through **`Access`** when Naga wraps loads.                                      |
| `lower_stmt`      | Implements **`Store`** when **pointer** is **`Access`** (and nested **`Load(Access)`** if Naga emits it).                                                        |
| Filetests         | Corpus in `expected-passing-tests.md`; remove **`@unimplemented`** when all three targets pass or mark **`@unsupported`** with reason (e.g. trap semantics TBD). |

## Decisions (from `00-notes.md`)

- **Targets:** `jit.q32`, `wasm.q32`, `rv32.q32` for the listed corpus.
- **Matrices:** Include **column** vector operations, not only scalar elements.
- **Access strategy:** Select chains for small fixed arity; stack path only if required.

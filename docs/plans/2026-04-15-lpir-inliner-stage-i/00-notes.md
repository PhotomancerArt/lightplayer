# Plan notes — `lpir-inliner` stage i (M0 stable `CalleeRef`)

## Scope of work

Implement the **M0 — Stable CalleeRef refactor** from
`docs/roadmaps/2026-04-15-lpir-inliner/m0-stable-callee-ref.md`:

- Replace flat `CalleeRef(pub u32)` (imports first, then locals in one index space) with a typed enum `CalleeRef::Import(ImportId)` / `CalleeRef::Local(FuncId)`.
- Add `ImportId(u16)` and `FuncId(u16)` with stable identity (safe for future dead-function elimination).
- Update `lpir` (types, module, builder, parse, print, validate, interp, tests) and downstream crates (`lpvm-native`, `lpvm-wasm`, `lps-frontend`) per the roadmap.
- **No intended behavior change**: same IR semantics and test expectations; mechanical migration off index arithmetic.

Out of scope for this stage: inliner, `Block` ops, filetest `@config`, dead-function elimination (later milestones).

## Current state of the codebase (relevant to this scope)

- **Layout**: LPIR lives under `lp-shader/lpir/` (not the repo root crate name alone).
- **`CalleeRef`**: `lp-shader/lpir/src/types.rs` defines `pub struct CalleeRef(pub u32)` with comment “imports first, then local functions”.
- **`LpirModule`**: `lp-shader/lpir/src/lpir_module.rs` holds `imports: Vec<ImportDecl>` and `functions: Vec<IrFunction>`. Helpers `callee_ref_import`, `callee_ref_function`, `callee_as_import`, `callee_as_function` implement the flat index split.
- **`ModuleBuilder`**: `add_import` / `add_function` return `CalleeRef` using the same flat encoding (`lp-shader/lpir/src/builder.rs`).
- **`IrFunction`**: has `name`, `is_entry`, `vmctx_vreg`, params, body, etc.; **no** `FuncId` field today.
- **Consumers**: `CalleeRef` appears in `print`, `parse`, `validate`, `interp` (uses `callee_as_import` / `callee_as_function`), `lpvm-native` `lower.rs`, `lpvm-wasm` `emit/ops.rs` and `emit/imports.rs`, `lps-frontend` `lower.rs` / `lower_ctx.rs` / `lower_lpfx.rs`, tests in `lpir/src/tests/validate.rs`. **`lpvm-cranelift` has no `CalleeRef` string matches** in a quick grep — may not need changes for M0.
- **Roadmap validation commands** assume workspace crates; commands should be run from the workspace that contains `lp-shader` members (see root `Cargo.toml` / workspace structure when validating).

## Questions (planning)

Answers will be appended below as we resolve them in chat.

| # | Question | Status |
|---|----------|--------|
| 1 | How should `LpirModule` store local functions so `FuncId` stays stable across future deletion without renumbering `Call` sites? | **Resolved** |
| 2 | Should each `IrFunction` store a `func_id: FuncId` field (redundant with map keys), or only the `BTreeMap` key? | **Resolved** |
| 3 | Imports: keep `Vec<ImportDecl>` + `ImportId` as vec index vs symmetric map? | **Resolved** |

### Suggested directions (for discussion)

- **Storage**: Options include `(a)` `Vec<IrFunction>` with `FuncId` **not** equal to vec index + side map `FuncId -> usize`, `(b)` `BTreeMap<FuncId, IrFunction>`, `(c)` `Vec<Option<IrFunction>>` with `FuncId` as slot index (sparse, deletion = `None`). Roadmap allows “simpler option” for small counts.
- **`IrFunction`**: Optional `func_id: FuncId` field for debugging and map-free reverse lookup — roadmap says “consider”.
- **Width**: Roadmap uses `u16` for ids; confirm vs existing counts (imports + functions) in largest modules.

## Answers (from chat)

### Q1 — Local function storage

**Answer:** Use **`BTreeMap<FuncId, IrFunction>`** (option 2).

**Implications:**

- Iteration order is sorted by **`FuncId`**, not insertion order. With monotonic id assignment (`0, 1, 2, …`), codegen order usually matches old `Vec` order; after deletes + new inserts, new ids should be ordered consistently if we allocate ids from a counter.
- All call sites and builders must construct **`CalleeRef::Local(FuncId)`** instead of flat indices.

### Q2 — `FuncId` on `IrFunction`

**Answer:** **No redundant field** — single source of truth: **`FuncId` only as the `BTreeMap` key**. APIs that need both pass **`(FuncId, &IrFunction)`** or look up with **`module.functions.get(&id)`**.

### Q3 — Import storage

**Answer:** Keep **`imports: Vec<ImportDecl>`** with **`ImportId(u16)`** equal to the **index** in that vector (same model as today, but typed). No `BTreeMap` for imports in M0.

## Notes

- **Cranelift / native / interp** iterate `module.functions` today as a `Vec`; they will iterate **`BTreeMap`** entries or collect sorted ids—small mechanical updates alongside `CalleeRef` migration.
- **Build granularity:** Intermediate steps do not need to keep `cargo check` green; only the **end of the plan** (phase 5 / full validation) must pass. Phases are logical slices, not per-commit merge requirements.

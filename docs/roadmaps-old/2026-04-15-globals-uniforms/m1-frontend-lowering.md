# Milestone 1: Frontend Lowering — Globals & Uniforms

## Goal

The frontend (`lps-frontend`) lowers Naga `GlobalVariable` declarations into
LPIR `Load`/`Store` ops against the VMContext buffer, and synthesizes a
`__shader_init()` function for global initializers. Module metadata carries
the globals/uniforms layout so the runtime knows allocation sizes and offsets.

## Suggested Plan Name

`globals-uniforms-m1`

## Scope

### In scope

- **Module metadata**: Add two `LpsType::Struct` values to `LpsModuleSig`:
  `uniforms_type` and `globals_type`. Each struct's members correspond to the
  declared uniforms / globals respectively. The existing layout infrastructure
  (`type_size`, `type_alignment`, `offset_for_path`) computes sizes, offsets,
  and supports path-based access. `LpvmDataQ32::new(uniforms_type)` gives the
  host a typed byte buffer for setting uniforms. No custom metadata table
  needed — the storage qualifier distinction is between the two structs, not
  per-field.

- **Layout computation during lowering**: Walk `naga::Module::global_variables`.
  For each global, map Naga type → `LpsType`, compute std430 offset using
  existing `lps-shared::layout` functions. Uniforms are laid out first
  (offsets starting at `VMCTX_HEADER_SIZE`), then globals (offsets starting
  at `VMCTX_HEADER_SIZE + uniforms_size`).

- **Expression lowering**: Handle `Expression::GlobalVariable(handle)` in
  `lower_expr.rs`. For reads: emit `Load { dst, base: VMCTX_VREG, offset }`.
  For multi-component types (vec, mat): emit multiple loads or use the
  existing multi-word load pattern.

- **Statement lowering**: Handle `Statement::Store` targeting a global variable
  pointer in `lower_stmt.rs`. Emit `Store { base: VMCTX_VREG, offset, value }`.

- **`__shader_init()` synthesis**: Generate an LPIR function named
  `__shader_init` with no user parameters (just vmctx). For each global with
  an initializer, emit the initializer expression evaluation followed by
  stores to the global's offset. Constant initializers become literal loads +
  stores. Uniform-dependent initializers emit loads from the uniform region.

- **Naga address space filtering**: Only handle `AddressSpace::Uniform` and
  `AddressSpace::Private` (plain globals). Other address spaces (`Storage`,
  `WorkGroup`, `Handle`, etc.) produce clear `LowerError::Unsupported` errors.

- **Unit tests**: LPIR dump tests showing correct Load/Store offsets for
  simple globals and uniforms. Verify `__shader_init` is synthesized with
  expected ops.

### Out of scope

- Runtime allocation / execution (Milestone 2).
- Engine integration (Milestone 3).
- Filetests actually running (they'll still be `@unimplemented` until M2 wires
  up the runtime).
- `in`/`out`/`buffer`/`shared` storage qualifiers.

## Key Decisions

- No new LPIR ops. Global/uniform access is `Load`/`Store` with
  `base: VMCTX_VREG` and concrete byte offsets baked in during lowering.

- `__shader_init` is a regular LPIR function, not special-cased in codegen.
  It has `is_entry: false` (or a new flag — up to implementation).

- The metadata is two `LpsType::Struct` values on `LpsModuleSig` (not
  `LpirModule`) since it's module-level metadata about the compilation, not
  IR instructions. Reusing `LpsType::Struct` means `LpvmDataQ32`, path
  resolution, and layout computation all work out of the box.

## Deliverables

- Updated `lps-frontend/src/lower.rs` — global variable traversal, layout
  computation, `__shader_init` synthesis.
- Updated `lps-frontend/src/lower_expr.rs` — `GlobalVariable` expression
  handling.
- Updated `lps-frontend/src/lower_stmt.rs` — store-to-global handling.
- Extended `LpsModuleSig` (in `lps-shared`) with `uniforms_type` and
  `globals_type` (`LpsType::Struct` values).
- Unit tests for LPIR output with globals.

## Dependencies

None — this is the first milestone.

## Estimated Scope

~400-600 lines of new/changed code in `lps-frontend`, ~50-100 lines in
`lps-shared` for metadata types.

## Agent Execution Notes

This milestone is suitable for a single agent session. The agent should:

1. Read `lps-frontend/src/lower.rs`, `lower_expr.rs`, `lower_stmt.rs` to
   understand the current lowering pipeline.
2. Read `lps-shared/src/sig.rs` for `LpsModuleSig` structure.
3. Read `lps-shared/src/layout.rs` for layout computation functions.
4. Read existing Naga `GlobalVariable` / `AddressSpace` types (in naga crate
   docs or source) to understand what the frontend receives.
5. Implement the metadata types first, then lowering, then `__shader_init`.
6. Run `cargo check -p lps-frontend` and `cargo test -p lps-frontend` to
   verify compilation and existing tests still pass.
7. Add unit tests that compile GLSL with globals and verify the LPIR output
   contains correct Load/Store offsets.

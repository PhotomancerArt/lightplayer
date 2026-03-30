# Stage II plan summary

## Completed

- `lp-glsl-naga`: new `lower_access.rs` — dynamic `Expression::Access` loads (vector/matrix column, inout pointer args), nested `Access` matrix element reads, `Store` through `Access`, dynamic column / element matrix stores (`merge_flat_index_store`, `store_matrix_column_dynamic`).
- `lower_stmt.rs`: matrix **`AccessIndex`** column store (constant column) via direct `Copy` into flat layout; `Store` dispatches `Access` to `store_through_access`.
- `lower_expr.rs`: `Load` peels `Access`; nested `Access` uses `ValuePointer` result typing; delegates to `lower_access`.
- `expr_scalar.rs`: `expr_type_inner` for `Expression::Access` (mirrors `AccessIndex` without const index checks).
- Filetests: removed file-level `// @unimplemented(backend=wasm)` where **jit / wasm / rv32** all pass on the 13-file corpus below.

## Corpus (150 tests, 13 files) — all pass on `jit.q32`, `wasm.q32`, `rv32.q32`

`matrix/mat{2,3,4}/incdec-matrix-{element,column}.glsl`, `operators/incdec-matrix-{element,column}.glsl`, `vec/bvec2/assign-element.glsl`, `vec/bvec2/index-variable-valid.glsl`, `vec/bvec{2,3,4}/access-array.glsl`.

## Deferred / follow-ups

- `operators/incdec-matrix.glsl`: whole-matrix postfix increment/decrement semantics vs expectations.
- `vec/bvec2/index-variable-bounds.glsl` and trap semantics (Tier B in plan).

## Validation record

- `jit.q32`: 150/150 (13 files)
- `wasm.q32`: 150/150
- `rv32.q32`: 150/150
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`

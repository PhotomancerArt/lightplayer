# Phase 3: Matrix element stores and matrix builtins

## Scope of phase

- Remove or implement the **matrix component store** path currently rejected in `lower_stmt.rs`
  (`component store to matrix element not supported`).
- Ensure **matrix builtins** used by filetests (`transpose`, `inverse`, `determinant`,
  `outerProduct`, `matrixCompMult`, etc.) lower and execute on **jit.q32** once invoke supports
  return sizes (phase 4 may still be required for large returns).

## Code organization reminders

- Reuse existing `lower_math.rs` matrix decomposition where possible; avoid duplicating linear
  algebra formulas.
- If a builtin is partially implemented, add a single `TODO` at the gap, not scattered comments.

## Implementation details

1. **`lower_stmt.rs`** — handle `Store` into matrix elements (Naga’s `AccessIndex` chain on
   matrix lvalues). Match GLSL column-major layout and existing scalarized VReg layout.

2. **`lower_math.rs` / `lower_expr.rs`** — verify matrix builtin coverage against
   `filetests/builtins/matrix-*.glsl` and `matrix/mat*/fn-transpose.glsl` etc.

3. **Operators** — `matrix/**` and `operators/incdec-matrix-*.glsl` depend on stores and metadata;
   run incremental filetests as you go.

4. **Tests**

```bash
./scripts/filetests.sh matrix/mat2/op-assign.glsl matrix/mat2/incdec-matrix-element.glsl
./scripts/filetests.sh builtins/matrix-compmult.glsl builtins/matrix-inverse.glsl
```

## Validate

```bash
cargo test -p lps-frontend
cargo test -p lps-filetests
./scripts/filetests.sh matrix/
```

(Full matrix tree may stay red until phase 4 completes invoke for `mat3`/`mat4`.)

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

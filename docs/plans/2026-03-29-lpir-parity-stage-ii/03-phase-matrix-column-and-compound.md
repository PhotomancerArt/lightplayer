# Phase 3: Matrix column + compound inc/dec

## Scope of phase

- **Column** vector **load** / **store** on matrix locals (`m[col]` as `vecN`, including `m[col]++` style lowering).
- Complete **read-modify-write** paths for **increment / decrement** when Naga lowers them to **load + binop + store** through **`Access`**.

## Code organization reminders

- Reuse **`naga_type_to_ir_types`** / **`vector_size_usize`** for column width.
- Column store = **multiple `Copy`** into `col * rows + r` for `r in 0..rows`.

## Implementation details

1. **Remove or replace** the **`store to matrix column not supported`** error for **`Access`** / **`AccessIndex`** shapes that correspond to a **full column** write when the RHS is a **vector** of matching size.

2. **`lower_expr`:** `Access` on matrix local → return **`VRegVec`** of length **rows** (column vector).

3. **`lower_stmt`:** Store to **column pointer** → write **rows** scalars into the flat layout.

4. **Compound updates:** If failures remain after phases 1–2, trace whether the **store pointer** is **`Access`**, **`Load`**, or **`AccessIndex`**; add the minimal matching arm so **incdec-matrix-element** and **incdec-matrix-column** compile.

### Tests

- `matrix/mat2/incdec-matrix-element.glsl`
- `matrix/mat3/incdec-matrix-element.glsl`
- `matrix/mat4/incdec-matrix-element.glsl`
- `matrix/mat2/incdec-matrix-column.glsl` (and mat3/mat4)
- `operators/incdec-matrix*.glsl` as listed in [`expected-passing-tests.md`](./expected-passing-tests.md)

## Validate

```bash
bash scripts/glsl-filetests.sh --target jit.q32 matrix/mat2/incdec-matrix-element.glsl matrix/mat2/incdec-matrix-column.glsl
bash scripts/glsl-filetests.sh --target jit.q32 operators/incdec-matrix-element.glsl operators/incdec-matrix-column.glsl
```

Extend to mat3/mat4 paths when mat2 is green.

# Phase 2: `Store` through `Access` (vectors + matrix elements)

## Scope of phase

Handle **`Statement::Store`** when the **pointer** is **`Expression::Access`** (and any \* \*`Load(Access…)`\*\* wrapper Naga uses) for:

- **Vector / bvec / ivec** single-component assign (`a[k] = expr`).
- **Matrix element** assign when expressed as **`Access`** (not only nested **`AccessIndex`**).

## Code organization reminders

- Mirror **read** helpers: shared **“resolve base vregs + lane index”** logic avoids drift between
  load and store.
- Keep **`coerce_assignment_vregs`** as the single coercion path for the RHS.

## Implementation details

1. **`lower_stmt.rs` — `Statement::Store`**
   - Add arms (order narrow → broad):
     - `Store { pointer: Access { base, index }, value }` with **`base`** resolving to a **vector
       ** local → read all lanes (or merge with existing vregs), compute **index** vreg, emit \*
       \*per-lane selects** so exactly one lane gets the new scalar (others unchanged), **`Copy`**
       back to **`resolve_local`\*\* vregs.
     - If Naga uses **`Store(Load(Access…))`** or similar, peel **`Load`** the same way as \* \*`lower_expr`\*\*.

2. **Matrix element**
   - When **`Access`** tree matches **element** (column + row or flat index per audit), compute \*
     \*flat_i** and **`Copy`** to `dsts[flat_i]` like existing nested **`AccessIndex`\*\* arm.

3. **Constants vs dynamic index**
   - Literal index can fold to a single **`Copy`** (optimization); not required for correctness.

### Tests

- `vec/bvec2/assign-element.glsl` — all `// run:` on **jit.q32**.
- `matrix/mat2/incdec-matrix-element.glsl` — may still fail until phase 3 if compound ops split
  differently; at minimum **direct** `m[0][0] = x` if a micro-test exists.

## Validate

```bash
cargo test -p lps-frontend
bash scripts/filetests.sh --target jit.q32 vec/bvec2/assign-element.glsl
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

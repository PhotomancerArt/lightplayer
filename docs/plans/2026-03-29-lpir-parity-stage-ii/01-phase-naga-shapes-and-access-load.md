# Phase 1: Naga shapes audit and `Access` loads

## Scope of phase

Confirm the exact **Naga** expression trees for failing cases (matrix `++`, `bvec[i]`, `Load(Access…)`), then implement **read** lowering for **`Expression::Access`** on **local** vectors and matrices (and extend **`Load`** peeling as needed).

## Code organization reminders

- Prefer a small **`lower_access`** section or module: **one concept per function** (vector scalar pick vs matrix column vs matrix element).
- Entry points / `match` arms **first**; helpers **at the bottom**.
- **`TODO`**: only for deferred trap semantics or Naga version gaps.

## Implementation details

1. **Audit** (can be a short comment block in code or notes in `00-notes.md` after `DEBUG=1` runs):
   - `matrix/mat2/incdec-matrix-element.glsl` — pointer expression for `Store` (likely `Access` / `Load` combo).
   - `vec/bvec2/assign-element.glsl` — `a[0] =` vs `a.x =`.

2. **`expr_scalar.rs`**
   - Ensure **`Expression::Access`** has correct **`TypeInner`** (scalar component from vector; column vector from matrix; element scalar from column `Access`).

3. **`lower_expr.rs`**
   - **`Expression::Access { base, index }`**:
     - **Vector / bvec / ivec:** `lower_expr_vec(base)`, `ensure_expr(index)` as **I32**, **select chain** (`ieq` + `select`) for sizes 2–4.
     - **Matrix local:** first index → **column** = slice `col * rows .. col * rows + rows`; second index on column rvalue → **scalar** (or combine in one `Access` if Naga flattens — match audit).
   - **`Load { pointer }`:** if `pointer` is **`Access`**, lower as **load of accessed value** (delegate to same helpers).

4. **Bounds:** For **Tier A** valid-index tests, no trap required initially. Do **not** silently read past bounds — if index is unknown, either emit checks consistent with project policy or document gap in Tier B.

### Tests

- `cargo test -p lp-glsl-naga`
- `bash scripts/glsl-filetests.sh --target jit.q32 vec/bvec2/index-variable-valid.glsl` (partial pass acceptable if stores still fail — phase 2 completes).

## Validate

```bash
cargo test -p lp-glsl-naga
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

```bash
bash scripts/glsl-filetests.sh --target jit.q32 vec/bvec2/index-variable-valid.glsl
```

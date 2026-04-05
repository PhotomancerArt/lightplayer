# Phase 1: `expr_type_inner` and `expr_scalar_kind` for `Relational`

## Scope of phase

Add correct type inference for `Expression::Relational` in `lps-frontend/src/expr_scalar.rs` so no
valid GLSL relational hits `expr_type_inner unsupported …`.

## Code organization reminders

- Prefer one conceptual block per match arm; keep `Relational` next to other unary-ish result types.
- Helper `relational_result_type_inner(...)` at the **bottom** of the file if the match body gets
  long.
- TODO only for genuinely deferred work (e.g. Naga adds a new `RelationalFunction`).

## Implementation details

### `expr_type_inner`

Add `Expression::Relational { fun, argument }`:

| `RelationalFunction` | Result `TypeInner`                                                                                                                                                                                                                                      |
|----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `All`, `Any`         | `TypeInner::Scalar` with `ScalarKind::Bool`, width 4 (same as other bool scalars in this module).                                                                                                                                                       |
| `IsNan`, `IsInf`     | Same **vector size** as the float argument, **bool** element type. If argument is scalar float → scalar bool. Use `expr_type_inner` on `argument` and map `Vector { size, scalar }` → `Vector { size, bool_scalar }`; `Scalar(float)` → `Scalar(bool)`. |
| `Not`                | If Naga exposes it: same shape as argument, **bool** lanes (bvecN → bvecN). Verify enum in your Naga version; if `Not` is not `Relational`, skip and document in `00-notes.md`.                                                                         |

### `expr_scalar_kind`

- **`All` / `Any`:** return `Ok(ScalarKind::Bool)` (result is a **scalar** bool).
- **`IsNan` / `IsInf`:** callers asking for a **single** scalar kind on a vector result are a smell;
  ensure call sites use `expr_type_inner` for vector shape. If something still calls
  `expr_scalar_kind` on `isnan(vec)`, return `Bool` **or** refactor that call site to use
  `expr_type_inner` (prefer refactor if it fixes a real bug).
- **`Not`:** `Bool` if scalar; for vector, same rule as above.

### Tests

- `cargo test -p lps-frontend` (existing module tests).
- Optional: add a **small** unit test in `expr_scalar.rs` `mod tests` with a tiny `Function` + arena
  snippet if the crate already patterns that way; otherwise rely on filetests in phase 4.

## Validate

```bash
cd lps && cargo test -p lps-frontend && cargo check -p lps-frontend
```

```bash
cd lp2025 && cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

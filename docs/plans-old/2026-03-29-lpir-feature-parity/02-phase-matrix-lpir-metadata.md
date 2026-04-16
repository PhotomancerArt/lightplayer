# Phase 2: Matrix types — LPIR metadata and module signatures

## Scope of phase

Allow matrix types in **exported** function metadata and compilation entry points so that files
no longer fail with `unsupported type: Matrix { … }` before meaningful lowering runs. This
includes:

- `GlslType` (or equivalent) in `lpir` / `lps-frontend` for `mat2`, `mat3`, `mat4`
- `naga_type_inner_to_glsl` / `extract_functions` accepting matrix parameters and returns
- Consistency with existing **scalarized** matrix representation in lowering (column-major
  flattened `f32` VRegs)

## Code organization reminders

- Keep `glsl_metadata` and Naga-facing mapping in sync; avoid duplicating shape logic — one place
  for “matCxR → N scalar words”.
- Abstract things first (type enum + helpers), then wire `compile()` / metadata export.

## Implementation details

1. **`lpir/src/glsl_metadata.rs`** — add matrix variants (`Mat2`, `Mat3`, `Mat4` or a single
   parameterized form). Document column-major component order to match GLSL / Naga.

2. **`lps-frontend/src/lib.rs`** (and any `extract_functions` helpers) — extend
   `naga_type_inner_to_glsl` to map `TypeInner::Matrix` to the new `GlslType` / metadata shapes.

3. **Lowering** — verify matrix **locals** and **expressions** already scalarize; fix any mismatch
   between new metadata and `lower_ctx::naga_type_to_ir_types` for matrices used only inside
   functions vs at module boundary.

4. **Tests**
    - Minimal unit test: compile a shader whose **only** exported function returns `mat2` and
      call through the test harness if invoke already supports 4 words; otherwise defer execution
      to phase 4.
    - `cargo test -p lpir` if metadata is validated there.

5. **Out of scope for this phase:** matrix **element store** (phase 3); **invoke** beyond what
   already works (phase 4).

## Validate

```bash
cargo test -p lpir
cargo test -p lps-frontend
cargo test -p lps-filetests
```

Targeted filetests once a matrix file compiles past the type error (may still fail until phases
3–4):

```bash
./scripts/glsl-filetests.sh matrix/mat2/from-scalar.glsl
```

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

(if `lps-frontend` / `lpir` public API changes affect the firmware graph)

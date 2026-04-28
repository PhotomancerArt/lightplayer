# Phase 1: Make SignatureBuilder Numeric-Aware

## Current state

`SignatureBuilder` uses `Type::to_cranelift_type()` to get CLIF types.
For `Type::Float`, this returns `types::F32`. All float parameters and
returns are hardcoded to F32.

## Changes

Add a `scalar_float_type: IrType` parameter to the methods that emit
float-typed params/returns. In Q32 mode this is `types::I32`, in float
mode it's `types::F32`. Callers get this from `numeric.scalar_type()`.

### Updated signatures

```rust
pub fn build_with_triple(
    return_type: &Type,
    parameters: &[Parameter],
    pointer_type: IrType,
    triple: &Triple,
    scalar_float_type: IrType,  // NEW
) -> Signature

pub fn build(
    return_type: &Type,
    parameters: &[Parameter],
    pointer_type: IrType,
    scalar_float_type: IrType,  // NEW
) -> Signature
```

### Internal changes

In `add_type_as_params`:

- Vector base type: if `base_ty == Type::Float`, use `scalar_float_type`
  instead of `base_ty.to_cranelift_type()`.
- Matrix elements: use `scalar_float_type` instead of
  `Type::Float.to_cranelift_type()`.
- Scalar float: if `ty == Type::Float`, use `scalar_float_type`.

In `add_type_as_returns`:

- Scalar return: if `ty == Type::Float`, use `scalar_float_type`.
- Vector/matrix returns use StructReturn (pointer) — unchanged.

### Where the type is NOT changed

- `Type::Int` → `types::I32` (unchanged)
- `Type::UInt` → `types::I32` (unchanged)
- `Type::Bool` → `types::I8` (unchanged)
- Pointer types (out/inout, arrays) → `pointer_type` (unchanged)
- StructReturn → `pointer_type` (unchanged)

Only `Type::Float` and float-based vector/matrix components change.

### count_parameters / count_returns

These don't emit types, just count slots. No changes needed.

## Callers to update

Every call to `SignatureBuilder::build` or `build_with_triple` needs to
pass `scalar_float_type`. In the current codebase:

- `compile_function_to_clif_impl` in `glsl_compiler.rs`
- `glsl_jit_streaming` in `frontend/mod.rs` (builds float sig before
  transform — this becomes the final sig)
- Any other callers (search for `SignatureBuilder::build`)

For float mode: pass `types::F32`.
For Q32 mode: pass `types::I32` (or `numeric.scalar_type()`).

## Validate

```bash
cargo check -p lps-compiler --features std
scripts/filetests.sh
```

Existing callers all pass `types::F32` — no behavioral change yet.

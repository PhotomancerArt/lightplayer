# Phase 1: Format-Aware Builtin Declaration

## Problem

`declare_builtins()` declares all 98 builtins regardless of numeric mode.
In Q32 streaming mode, ~24 F32-only LPFX builtins are never called.
Each `declare_function` call allocates module-internal storage for the
function name, signature, and declaration entry. At peak, these are
16,790 bytes across 398 allocs (~42 bytes per alloc, ~170 bytes per
declaration including string + signature + hashmap entry).

## Current code

```rust
// backend/builtins/registry.rs
pub fn declare_builtins<M: Module>(module: &mut M, pointer_type: Type) -> Result<(), GlslError> {
    for builtin in BuiltinId::all() {
        module.declare_function(builtin.name(), Linkage::Import, &builtin.signature(pointer_type))?;
    }
    Ok(())
}
```

All 98 builtins declared unconditionally. In Q32 mode, the F32 variants
of LPFX functions (LpfnSnoise1F32, LpfnFbm2F32, etc.) are never used.
Similarly, in a hypothetical float mode, LpQ32* builtins would be unused.

## Fix

Add a `DecimalFormat` parameter to `declare_builtins` and skip builtins
that don't match the format.

Add a method to `BuiltinId` indicating its format affinity:

```rust
impl BuiltinId {
    pub fn format(&self) -> Option<DecimalFormat> {
        match self {
            BuiltinId::LpQ32Add | ... => Some(DecimalFormat::Q32),
            BuiltinId::LpfnSnoise1F32 | ... => Some(DecimalFormat::Float),
            BuiltinId::LpfnHash1 | ... => None, // format-agnostic
            BuiltinId::LpfnSnoise1Q32 | ... => Some(DecimalFormat::Q32),
        }
    }
}
```

Then filter in `declare_builtins`:

```rust
pub fn declare_builtins<M: Module>(
    module: &mut M,
    pointer_type: Type,
    format: DecimalFormat,
) -> Result<(), GlslError> {
    for builtin in BuiltinId::all() {
        if let Some(f) = builtin.format() {
            if f != format { continue; }
        }
        module.declare_function(builtin.name(), Linkage::Import, &builtin.signature(pointer_type))?;
    }
    Ok(())
}
```

## Scope

- `backend/builtins/registry.rs`: add `format()` method, update
  `declare_builtins` and `declare_for_jit` / `declare_for_object` signatures
- `backend/module/gl_module.rs`: pass format to `declare_builtins` in
  `new_jit` and `new_object` constructors. This requires knowing the format
  at module creation time — add `DecimalFormat` to `new_jit` / `new_object`
  or to the Target.
- `frontend/mod.rs`: pass format when creating modules
- `builtins-gen-app`: update if it calls declare_builtins
- Symbol lookup in JIT: the lookup closure iterates `BuiltinId::all()`;
  needs same filter, or can remain as-is (returns None for undeclared names)

## Expected savings

~24 unused F32 builtins × ~170 bytes ≈ ~4 KB at peak.

## Risk

Low. The filter is simple and testable. If a shader somehow references
a filtered-out builtin, compilation will fail with a clear error (function
not declared), caught by filetests.

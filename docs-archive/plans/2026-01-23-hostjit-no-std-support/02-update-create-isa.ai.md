# Phase 2: Update `create_isa()` to Use Helper

## Description

Modify `Target::create_isa()` for HostJit to use the new `create_host_isa()` helper instead of directly calling `cranelift_native`.

## Implementation

1. Update `create_isa()` match arm for `Target::HostJit`
2. Replace direct `cranelift_native::builder()` call with `create_host_isa(_flags.clone())`
3. Remove the `#[cfg(feature = "std")]` block and `#[cfg(not(feature = "std"))]` error return
4. Keep the caching logic (if isa.is_none(), create it)

## Success Criteria

- `create_isa()` works in both std and no_std modes
- Caching still works correctly
- No breaking changes to existing code

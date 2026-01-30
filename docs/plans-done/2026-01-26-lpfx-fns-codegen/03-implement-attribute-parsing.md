# Phase 3: Implement attribute parsing

## Description

Parse `#[lpfx_impl(...)]` attributes to extract variant type (if any) and GLSL signature string.

## Implementation

1. Create `lp-builtin-gen/src/lpfx/parse.rs`
2. Implement `parse_lpfx_attribute()` function:
   - Parse attribute using `syn::Attribute::parse_args()`
   - Extract optional variant identifier (`f32` or `q32`)
   - Extract GLSL signature string literal
   - Return `LpfxAttribute` structure
3. Handle both forms:
   - `#[lpfx_impl("signature")]` - non-decimal
   - `#[lpfx_impl(variant, "signature")]` - decimal

## Success Criteria

- Correctly parses non-decimal attributes
- Correctly parses decimal attributes with f32/q32 variants
- Returns appropriate error for invalid syntax
- Code compiles

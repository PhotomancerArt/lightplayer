# Phase 3: Implement attribute parsing

## Description

Parse `#[lpfn_impl(...)]` attributes to extract variant type (if any) and GLSL signature string.

## Implementation

1. Create `lps-builtin-gen-app/src/lpfn/parse.rs`
2. Implement `parse_lpfn_attribute()` function:
    - Parse attribute using `syn::Attribute::parse_args()`
    - Extract optional variant identifier (`f32` or `q32`)
    - Extract GLSL signature string literal
    - Return `LpfnAttribute` structure
3. Handle both forms:
    - `#[lpfn_impl("signature")]` - non-decimal
    - `#[lpfn_impl(variant, "signature")]` - decimal

## Success Criteria

- Correctly parses non-decimal attributes
- Correctly parses decimal attributes with f32/q32 variants
- Returns appropriate error for invalid syntax
- Code compiles

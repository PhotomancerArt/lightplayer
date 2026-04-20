# Phase 9: Update existing LPFX functions with attributes

## Description

Add `#[lpfn_impl(...)]` attributes to all existing LPFX function implementations.

## Implementation

1. Update hash functions (`hash.rs`):
   - Add `#[lpfn_impl("u32 lpfn_hash1(u32 x, u32 seed)")]` etc.
2. Update simplex functions:
   - Add `#[lpfn_impl(f32, "float lpfn_snoise1(float x, u32 seed)")]` to f32 implementations
   - Add `#[lpfn_impl(q32, "float lpfn_snoise1(float x, u32 seed)")]` to q32 implementations
   - Repeat for simplex2 and simplex3
3. Verify all functions have correct attributes

## Success Criteria

- All LPFX functions have `#[lpfn_impl(...)]` attributes
- Attributes have correct syntax
- Codegen can discover all functions
- Code compiles

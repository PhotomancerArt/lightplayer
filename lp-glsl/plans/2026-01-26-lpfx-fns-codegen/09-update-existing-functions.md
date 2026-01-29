# Phase 9: Update existing LPFX functions with attributes

## Description

Add `#[lpfx_impl(...)]` attributes to all existing LPFX function implementations.

## Implementation

1. Update hash functions (`hash.rs`):
   - Add `#[lpfx_impl("u32 lpfx_hash1(u32 x, u32 seed)")]` etc.
2. Update simplex functions:
   - Add `#[lpfx_impl(f32, "float lpfx_snoise1(float x, u32 seed)")]` to f32 implementations
   - Add `#[lpfx_impl(q32, "float lpfx_snoise1(float x, u32 seed)")]` to q32 implementations
   - Repeat for simplex2 and simplex3
3. Verify all functions have correct attributes

## Success Criteria

- All LPFX functions have `#[lpfx_impl(...)]` attributes
- Attributes have correct syntax
- Codegen can discover all functions
- Code compiles

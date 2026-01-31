# Phase 6: Pack/Unpack Functions

## Goal

Implement pack/unpack builtins for converting between float and uint bit patterns.

## Functions to Implement

- **packHalf2x16** - tests: `builtins/pack-half.glsl`
- **packDouble2x32** - tests: `builtins/pack-double.glsl`
- **packUnorm4x8** - tests: `builtins/pack-unorm.glsl`
- **unpackHalf2x16** - tests: `builtins/unpack-half.glsl`
- **unpackDouble2x32** - tests: `builtins/unpack-double.glsl`
- **unpackUnorm4x8** - tests: `builtins/unpack-unorm.glsl`

## Implementation Details

### Pack Functions

- **packHalf2x16**: Pack 2 float32 values into uint32 as half-precision floats
- **packDouble2x32**: Pack 2 float32 values into uint64 as double-precision (may not be needed for
  q32)
- **packUnorm4x8**: Pack 4 float32 values (0.0-1.0) into uint32 as 8-bit unorm values

### Unpack Functions

- **unpackHalf2x16**: Unpack uint32 into 2 float32 values from half-precision
- **unpackDouble2x32**: Unpack uint64 into 2 float32 values from double-precision
- **unpackUnorm4x8**: Unpack uint32 into 4 float32 values from 8-bit unorm

## Considerations

- These functions convert between float bit patterns and integers
- For fixed-point: may need to convert q32 → float → pack → uint, or work directly with bit patterns
- Half-precision: 16-bit float format (sign:1, exp:5, mantissa:10)
- Unorm: unsigned normalized integer (0-255 maps to 0.0-1.0)

## Files to Create/Modify

- `lightplayer/crates/lp-glsl-builtins/src/q32/pack_half.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/pack_double.rs` - new file (if needed)
- `lightplayer/crates/lp-glsl-builtins/src/q32/pack_unorm.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/unpack_half.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/unpack_double.rs` - new file (if needed)
- `lightplayer/crates/lp-glsl-builtins/src/q32/unpack_unorm.rs` - new file
- Run builtin generator to update boilerplate

## Success Criteria

- All pack/unpack functions implemented
- Unit tests pass
- Functions registered in builtin generator
- Code compiles without warnings
- Tests pass for all pack/unpack functions


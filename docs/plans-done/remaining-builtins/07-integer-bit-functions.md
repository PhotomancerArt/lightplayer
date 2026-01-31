# Phase 7: Integer Bit Functions (Evaluation and Implementation)

## Goal

Evaluate complexity and implement integer bit manipulation functions as inline or builtin based on
complexity.

## Functions to Evaluate and Implement

- **bitCount** - tests: `builtins/integer-bitcount.glsl`
- **bitfieldExtract** - tests: `builtins/integer-bitfieldextract.glsl` (already determined < 10
  instructions → inline)
- **bitfieldInsert** - tests: `builtins/integer-bitfieldinsert.glsl` (already determined < 10
  instructions → inline)
- **bitfieldReverse** - tests: `builtins/integer-bitfieldreverse.glsl`
- **findLSB** - tests: `builtins/integer-findlsb.glsl`
- **findMSB** - tests: `builtins/integer-findmsb.glsl`
- **imulExtended** - tests: `builtins/integer-imulextended.glsl`
- **uaddCarry** - tests: `builtins/integer-uaddcarry.glsl`
- **umulExtended** - tests: `builtins/integer-umulextended.glsl`
- **usubBorrow** - tests: `builtins/integer-usubborrow.glsl`

## Evaluation Criteria

- **< 10 instructions:** Implement as inline conversion in transform
- **>= 10 instructions or complex:** Implement as builtin function

## Implementation Details

### Already Determined (Inline)

- **bitfieldExtract**: ~3-4 instructions (shift + mask) → inline
- **bitfieldInsert**: ~5-6 instructions (mask + shift + OR) → inline

### To Evaluate

- **bitCount**: Popcount algorithm - evaluate complexity
- **bitfieldReverse**: Bit reversal - evaluate complexity
- **findLSB**: Find least significant bit - evaluate complexity
- **findMSB**: Find most significant bit - evaluate complexity
- **imulExtended**: Extended multiplication with high/low parts - evaluate complexity
- **uaddCarry**: Add with carry - evaluate complexity
- **umulExtended**: Unsigned extended multiplication - evaluate complexity
- **usubBorrow**: Subtract with borrow - evaluate complexity

## Files to Create/Modify

- For inline: `lightplayer/crates/lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`
- For builtins: `lightplayer/crates/lp-glsl-builtins/src/q32/` (new files as needed)
- Run builtin generator if any builtins are added

## Success Criteria

- All functions evaluated and implemented appropriately (inline or builtin)
- Unit tests pass
- Functions registered appropriately (inline in transform or builtin generator)
- Code compiles without warnings
- Tests pass for all integer bit functions


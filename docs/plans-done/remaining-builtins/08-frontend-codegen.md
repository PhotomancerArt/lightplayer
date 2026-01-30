# Phase 8: Frontend Codegen for Missing Functions

## Goal
Implement frontend codegen for functions that don't need q32 implementations.

## Functions to Implement
- **Integer bit functions** that are pure integer operations (tests referenced in Phase 7)
- **floatBitsToInt** - tests: `builtins/common-floatbitstoint.glsl`
- **intBitsToFloat** - tests: `builtins/common-intbitstofloat.glsl`
- Any other missing frontend implementations

## Implementation Details

### floatBitsToInt / intBitsToFloat
- Convert between float bit patterns and integers
- For fixed-point: may need special handling since we're using q32
- These are bit-pattern conversions, not value conversions
- File: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/builtins/common.rs`

### Integer Bit Functions (if not implemented as builtins)
- If any integer bit functions were determined to be frontend-only (pure integer operations)
- Implement in appropriate frontend codegen module
- File: `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/builtins/` (appropriate module)

## Files to Modify
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/builtins/common.rs` - add floatBitsToInt, intBitsToFloat
- `lightplayer/crates/lp-glsl-compiler/src/frontend/codegen/builtins/mod.rs` - wire up new functions
- Any other frontend codegen files as needed

## Success Criteria
- All frontend codegen functions implemented
- Tests pass
- Code compiles without warnings
- Functions properly integrated into builtin call dispatch


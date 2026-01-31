# Phase 3: Exponential Functions

## Goal

Implement inversesqrt builtin function.

## Functions to Implement

- **inversesqrt** (uses sqrt builtin: 1/sqrt(x)) - tests: `builtins/exp-inversesqrt.glsl`

## Implementation Details

### inversesqrt

- Formula: `inversesqrt(x) = 1 / sqrt(x)`
- Uses existing sqrt builtin: `__lp_q32_sqrt`
- Simple wrapper: call sqrt, then divide 1 by result
- Handle edge cases: x <= 0 (undefined, return 0 or max value)
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/inversesqrt.rs` (new file)

## Files to Create/Modify

- `lightplayer/crates/lp-glsl-builtins/src/q32/inversesqrt.rs` - new file
- Run builtin generator to update boilerplate

## Success Criteria

- Function implemented following existing patterns (see sqrt.rs, exp.rs)
- Unit tests pass with appropriate tolerance (3%)
- Function registered in builtin generator (auto-generated)
- Code compiles without warnings
- Tests pass: `scripts/glsl-filetests.sh builtins/exp-inversesqrt.glsl`


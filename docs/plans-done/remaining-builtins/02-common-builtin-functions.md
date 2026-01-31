# Phase 2: Common Builtin Functions

## Goal

Implement q32 builtins for common functions: mod, round, roundeven.

## Functions to Implement

- **mod** (uses div builtin) - tests: `builtins/common-mod.glsl`
- **round** (complex rounding algorithm) - tests: `builtins/common-round.glsl`
- **roundeven** (round to nearest even) - tests: `builtins/common-roundeven.glsl`

## Implementation Details

### mod

- Formula: `mod(x, y) = x - y * floor(x/y)`
- Uses existing div builtin
- Follow pattern from existing two-arg functions (atan2, pow)
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/mod.rs` (already exists, needs implementation)

### round

- Round to nearest integer (0.5 rounds up)
- Algorithm: similar to ceil/floor but with rounding
- Reference: fpm library `round()` implementation
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/round.rs` (new file)

### roundeven

- Round to nearest even integer (banker's rounding)
- Algorithm: round to nearest, but if exactly halfway, round to even
- Reference: fpm library or standard algorithm
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/roundeven.rs` (new file)

## Files to Create/Modify

- `lightplayer/crates/lp-glsl-builtins/src/q32/mod.rs` - implement mod function
- `lightplayer/crates/lp-glsl-builtins/src/q32/round.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/roundeven.rs` - new file
- Run builtin generator to update boilerplate

## Success Criteria

- All functions implemented following existing patterns (see sin.rs, exp.rs)
- Unit tests pass with appropriate tolerance (3% for complex functions)
- Functions registered in builtin generator (auto-generated)
- Code compiles without warnings
- Tests pass: `scripts/glsl-filetests.sh builtins/common-mod.glsl`, etc.


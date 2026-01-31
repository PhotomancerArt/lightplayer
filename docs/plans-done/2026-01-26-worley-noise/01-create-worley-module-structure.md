# Phase 1: Create Worley Module Structure

## Description

Create the `worley/` subdirectory under `lpfx/` and set up the module structure. This follows the
same pattern as the `simplex/` module.

## Implementation

### Files to Create

1. **`lp-glsl-builtins/src/builtins/lpfx/worley/mod.rs`**
    - Module file that will export the worley functions
    - Initially empty or with placeholder comments

### Files to Update

1. **`lp-glsl-builtins/src/builtins/lpfx/mod.rs`**
    - Add `pub mod worley;` to export the worley module

## Success Criteria

- `worley/` directory exists under `lpfx/`
- `worley/mod.rs` file exists
- `lpfx/mod.rs` exports the worley module
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`

## Notes

- Follow the same structure as `simplex/` module
- Keep module file simple - actual exports will be added as functions are implemented

# Phase 1: Create Directory Structure and Module Files

## Description

Create the directory structure for color/space and math modules, and set up the module files with
proper exports.

## Implementation

### Create Directories

- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/math/`

### Create Module Files

1. `lpfx/color/mod.rs` - Module declaration for color module
2. `lpfx/color/space/mod.rs` - Module declaration for color/space, will export HSV functions
3. `lpfx/math/mod.rs` - Module declaration for math module, will export saturate

### Update Root Module

Update `lpfx/mod.rs` to export the new `color` and `math` modules.

## Success Criteria

- Directory structure matches design
- Module files created with proper structure
- Root module exports new modules
- Code compiles

## Style Notes

### Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

### Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

### Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented

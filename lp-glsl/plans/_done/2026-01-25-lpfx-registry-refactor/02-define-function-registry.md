# Phase 2: Define Function Registry in lpfx_fns.rs

## Description

Create the const array of all LPFX functions in `lpfx_fns.rs`. This will be the single source of truth for all LPFX function definitions.

## Implementation

### File: `frontend/semantic/lpfx/lpfx_fns.rs`

Create const array `LPFX_FNS` containing all current functions:
- `lpfx_hash1` - (u32, u32) -> u32
- `lpfx_hash2` - (u32, u32, u32) -> u32
- `lpfx_hash3` - (u32, u32, u32, u32) -> u32
- `lpfx_snoise1` - (float, uint) -> float
- `lpfx_snoise2` - (vec2, uint) -> float
- `lpfx_snoise3` - (vec3, uint) -> float

For each function:
- Create `FunctionSignature` with proper name, parameter types, and return type
- Create `LpfxFnImpl` entries for each decimal format (q32 for simplex, None for hash)
- Set `builtin_module` and `rust_fn_name` appropriately

## Success Criteria

- All current functions defined in registry
- Function signatures are correct
- Implementations mapped correctly
- Code compiles
- Code formatted with `cargo +nightly fmt`

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
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented

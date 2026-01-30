# Phase 3: Update codegen tool to generate multiple entries for overloads

## Description

Update `lp-builtin-gen` to generate multiple `LpfxFn` entries for functions with the same GLSL name but different signatures. Add validation to ensure overloads have distinct parameter signatures.

## Implementation

### File: `lp-glsl/apps/lp-builtin-gen/src/lpfx/generate.rs`

**Update `generate_lpfx_fns`:**
- Currently groups functions by GLSL name and uses first function's signature
- Change to: For each group, create one `LpfxFn` entry per unique signature
- Each entry should map to the correct `BuiltinId` for its signature
- Update `format_lpfx_fn_impl` to handle individual function entries (not groups)

**Add signature comparison:**
- Helper function to compare function signatures (name + parameters)
- Use this to identify unique signatures within a name group

### File: `lp-glsl/apps/lp-builtin-gen/src/lpfx/validate.rs`

**Add validation:**
- Validate that overloads have distinct parameter signatures
- Error if two functions with same name have identical parameter signatures
- Check should happen after parsing but before generation

### Regenerate Builtins

After updating the codegen tool:
1. Run `scripts/build-builtins.sh` to regenerate `lpfx_fns.rs`
2. Verify that `lpfx_hsv2rgb` has both vec3 and vec4 entries
3. Check that all overloaded functions have multiple entries

## Success Criteria

- Codegen tool generates multiple `LpfxFn` entries for overloaded functions
- Each entry correctly maps to its `BuiltinId`
- Validation ensures distinct signatures for overloads
- `lpfx_hsv2rgb` generates both vec3 and vec4 entries
- `lpfx_fns.rs` regenerated successfully
- Code compiles without warnings
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

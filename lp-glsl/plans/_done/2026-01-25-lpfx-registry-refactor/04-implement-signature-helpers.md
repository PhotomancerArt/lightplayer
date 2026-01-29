# Phase 4: Implement Signature Conversion Helpers

## Description

Implement helper functions in `lpfx_sig.rs` for converting GLSL signatures to Cranelift signatures and handling argument expansion.

## Implementation

### File: `frontend/semantic/lpfx/lpfx_sig.rs`

Implement functions:
- `expand_vector_args(param_types: &[Type], values: &[Value]) -> Vec<Value>` - Expand vector arguments to individual components
- `convert_to_cranelift_types(param_types: &[Type], format: DecimalFormat) -> Vec<CraneliftType>` - Convert GLSL types to Cranelift types based on format (Float/UInt/Int → i32)
- `build_call_signature(fn: &LpfxFn, impl: &LpfxFnImpl, format: DecimalFormat) -> Signature` - Build Cranelift signature dynamically from function signature

Rules:
- Vectors expand to components (vec2 → 2 scalars, vec3 → 3 scalars)
- Float → i32 (q32)
- UInt → i32 (Cranelift representation)
- Int → i32
- Panic on unsupported types

## Success Criteria

- All helper functions implemented
- Vector expansion works correctly
- Type conversion based on DecimalFormat works
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

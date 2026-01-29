# Phase 8: Update Builtin Registry Integration

## Description

Update builtin registry to use the LPFX registry for name and signature mapping.

## Implementation

### File: `backend/builtins/registry.rs`

Replace:
- Hardcoded `BuiltinId::name()` match cases for LPFX functions with registry lookup
- Hardcoded signature match cases with registry-based signature building
- Use `find_lpfx_fn` and `get_impl_for_format` to get function info

## Success Criteria

- Builtin registry uses LPFX registry
- Function names and signatures come from registry
- No hardcoded LPFX function matches
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

# Phase 1: Extend LpLibFn with Q32 Mapping Methods

## Goal

Add methods to `LpLibFn` enum to determine if a function needs q32 mapping and what the mapped name
is. This keeps the source of truth in `LpLibFn` for conversion requirements.

## Tasks

### 1.1 Add `q32_name()` Method

In `lp-glsl/lp-glsl-compiler/src/frontend/semantic/lp_lib_fns.rs`:

- Add `q32_name(&self) -> Option<&'static str>` method to `LpLibFn` impl
- Return `Some("__lp_q32_lpfx_snoise1")` for `Simplex1`
- Return `Some("__lp_q32_lpfx_snoise2")` for `Simplex2`
- Return `Some("__lp_q32_lpfx_snoise3")` for `Simplex3`
- Return `None` for hash functions (they don't need q32 conversion)

### 1.2 Add `needs_q32_mapping()` Method

In the same file:

- Add `needs_q32_mapping(&self) -> bool` method
- Delegate to `q32_name().is_some()` to keep a single source of truth
- This returns `true` for simplex functions, `false` for hash functions

### 1.3 Add Tests

Add tests to verify:

- `LpLibFn::Simplex1.needs_q32_mapping()` returns `true`
- `LpLibFn::Simplex1.q32_name()` returns `Some("__lp_q32_lpfx_snoise1")`
- `LpLibFn::Hash1.needs_q32_mapping()` returns `false`
- `LpLibFn::Hash1.q32_name()` returns `None`

## Success Criteria

- `q32_name()` method exists and returns correct values
- `needs_q32_mapping()` delegates to `q32_name().is_some()`
- Tests pass
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented

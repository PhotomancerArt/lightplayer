# Phase 1: Add lp-model dependency to lp-shared

## Goal

Add `lp-model` as a dependency to `lp-shared` so that transport traits can use `ClientMessage` and `ServerMessage` types.

## Tasks

1. Update `lp-shared/Cargo.toml`:
   - Add `lp-model` dependency: `lp-model = { path = "../lp-model", default-features = false }`
   - Ensure it's added to dependencies section (not dev-dependencies)

2. Verify dependency:
   - Run `cargo check` in `lp-shared` to ensure it compiles
   - Verify no circular dependencies

## Success Criteria

- [ ] `lp-model` is added as a dependency in `lp-shared/Cargo.toml`
- [ ] `lp-shared` compiles with the new dependency
- [ ] No circular dependency issues
- [ ] Code compiles without warnings

## Style Notes

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Run `cargo +nightly fmt` on all changes before committing
- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete

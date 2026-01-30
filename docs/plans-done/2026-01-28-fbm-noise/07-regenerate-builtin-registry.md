# Phase 7: Regenerate builtin registry

## Description

Run the builtin generator to register all new functions in the builtin system.

## Implementation

Run the builtin generator:

```bash
cargo run --bin lp-builtin-gen --manifest-path lp-glsl/apps/lp-builtin-gen/Cargo.toml
```

Or use the build script:

```bash
scripts/build-builtins.sh
```

This will:

- Discover all new `__lpfx_*` functions with `#[lpfx_impl_macro::lpfx_impl]` attributes
- Generate entries in `lpfx_fns.rs`
- Generate entries in `BuiltinId` enum
- Update the builtin registry

## Success Criteria

- Builtin generator runs successfully
- All new functions appear in `lpfx_fns.rs`
- All new functions have `BuiltinId` enum variants
- Code compiles without errors

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented

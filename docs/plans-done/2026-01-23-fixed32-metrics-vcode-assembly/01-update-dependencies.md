# Phase 1: Update Dependencies and Module Types

## Description

Update `Cargo.toml` to enable the `emulator` feature for `lp-glsl-compiler`, and update type
signatures throughout the codebase to use `ObjectModule` instead of `JITModule`.

## Implementation

- Update `lp-glsl/lp-glsl-q32-metrics-app/Cargo.toml`:
    - Add `emulator` feature to `lp-glsl-compiler` dependency
- Update imports in `src/main.rs`:
    - Change from `cranelift_jit::JITModule` to `cranelift_object::ObjectModule`
- Update type signatures in `src/main.rs`:
    - Change `GlModule<JITModule>` to `GlModule<ObjectModule>` in function signatures
- Update type signatures in `src/stats.rs`:
    - Change `GlModule<JITModule>` to `GlModule<ObjectModule>`
- Update type signatures in `src/clif.rs`:
    - Change `GlModule<JITModule>` to `GlModule<ObjectModule>`

## Success Criteria

- `Cargo.toml` includes `emulator` feature
- All type signatures use `ObjectModule` instead of `JITModule`
- Code compiles without errors
- No warnings (except unused code that will be used in later phases)

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
- Use measured, factual descriptions of what was implemented

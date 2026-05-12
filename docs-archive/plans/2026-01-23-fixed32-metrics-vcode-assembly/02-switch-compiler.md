# Phase 2: Switch Compiler to ObjectModule and RISC-V Target

## Description

Update `compiler.rs` to use `compile_to_gl_module_object()` instead of `compile_to_gl_module_jit()`, and switch from `Target::host_jit()` to `Target::riscv32_emulator()` to generate RISC-V 32-bit code.

## Implementation

- Update `src/compiler.rs`:
  - Change `Target::host_jit()` to `Target::riscv32_emulator()`
  - Change `compile_to_gl_module_jit()` to `compile_to_gl_module_object()`
  - Update return type from `GlModule<JITModule>` to `GlModule<ObjectModule>`
  - Update error messages if needed

## Success Criteria

- Compiler uses RISC-V 32-bit target
- Compiler uses ObjectModule instead of JITModule
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
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented

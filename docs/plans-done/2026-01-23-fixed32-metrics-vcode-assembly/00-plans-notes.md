# Plan: Add VCode and Assembly Generation to Q32 Metrics

## Questions

### Q1: When should we compile functions to get vcode/assembly?

**Context**: Currently, `lp-glsl-q32-metrics-app` compiles GLSL to CLIF IR (before and after
transform) but doesn't actually compile the functions to machine code. To get vcode and assembly, we
need to actually compile the functions using `define_function`, which generates the compiled code.

**Answer**: We should compile functions after we have the CLIF IR but before we write the CLIF
files. This means:

- Compile functions in `module_before` to get vcode/assembly before transform
- Compile functions in `module_after` to get vcode/assembly after transform
- This happens in the `process_test` function, similar to how we currently write CLIF files
- **Important**: We need to ensure we're generating RISC-V 32-bit code (not host JIT), so we should
  use `Target::riscv32_emulator()` instead of `Target::host_jit()` in the compiler

### Q2: How do we access compiled_code from JITModule?

**Context**: Looking at `build_emu_executable` in `emu.rs`, it accesses `ctx.compiled_code()` after
calling `define_function`. However, `lp-glsl-q32-metrics-app` uses `JITModule`, not `ObjectModule`.
The JIT codegen in `jit.rs` doesn't currently capture vcode/disassembly.

**Answer**: We should **switch from JITModule to ObjectModule**. The emulator already does this
correctly:

- Use `compile_to_gl_module_object()` instead of `compile_to_gl_module_jit()`
- Use `GlModule<ObjectModule>` instead of `GlModule<JITModule>`
- Use `Target::riscv32_emulator()` (which creates ObjectModule)
- Then we can follow the same pattern as `build_emu_executable`:
    1. Create a context for each function
    2. Call `define_function` (which compiles the function)
    3. Access `ctx.compiled_code()` to get vcode and disassembly
    4. Extract the data before clearing the context
    5. Enable disassembly with `ctx.set_disasm(true)` before defining functions

### Q3: What format should vcode and assembly files use?

**Context**: Currently CLIF files are named `<function-name>.pre.clif` and
`<function-name>.post.clif`.

**Answer**:

- VCode files: `<function-name>.pre.vcode` and `<function-name>.post.vcode`
- Assembly files: `<function-name>.pre.s` and `<function-name>.post.s` (using `.s` extension for
  standard assembly format)

### Q4: How do we handle disassembly when Capstone isn't available?

**Context**: The `build_emu_executable` code shows that disassembly can fall back to vcode if
Capstone isn't available. However, vcode is not the same as assembly - it's an intermediate
representation.

**Answer**: Capstone should be available (via the `emulator` feature). We'll use the same pattern as
`build_emu_executable`:

- Try to generate real assembly using Capstone disassembler (preferred)
- If Capstone fails for some reason, fall back to vcode (same pattern as emulator code)
- The `emulator` feature will be required for the lp-glsl-q32-metrics-app app

### Q5: Should we add vcode/assembly sizes to the statistics?

**Context**: Currently statistics track CLIF size. It would be useful to also track vcode and
assembly sizes to see the bloat at different stages.

**Answer**: Yes, we should add:

- `vcode_size` to `FunctionStats` and `ModuleStats`
- `assembly_size` to `FunctionStats` and `ModuleStats`
- Deltas for both in `StatsDelta` (with percentage calculations)
- Include these in the report TOML files

This will allow comparing sizes at CLIF, vcode, and assembly levels.

### Q6: Do we need to compile functions for both before and after, or can we reuse compilation?

**Context**: Currently we compile GLSL twice - once for before transform, once for after transform.
To get vcode/assembly, we need to compile the CLIF functions to machine code.

**Suggested Answer**: We need to compile both `module_before` and `module_after` separately because:

- The CLIF IR is different (before vs after transform)
- The compiled code will be different
- We want to compare sizes at each stage

So we'll compile functions in both modules, similar to how we currently write CLIF files for both.

## Notes

- **RISC-V 32-bit target**: We need to ensure we're generating RISC-V 32-bit code, not host JIT
  code. This means changing `Target::host_jit()` to `Target::riscv32_emulator()` in `compiler.rs`.
  This requires the `emulator` feature to be enabled.

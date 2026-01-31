# lp-glsl-builtins-emu-app

Executable that links all builtin functions into a static library used for `lp-filetests` and
`lp-glsl-compiler`. Provides the entry point, panic handler, and ensures all builtin functions are
included in the binary.

## Overview

This application serves as the runtime foundation for compiled GLSL programs running in emulator
mode. It:

- Links all `__lp_q32_*` builtin functions from `lp-glsl-builtins`
- Provides the entry point (`_entry`) that initializes .bss and .data sections
- Implements the panic handler that reports errors to the host VM
- Ensures all builtin functions are included via generated references (prevents dead code
  elimination)

## Building

Build the static library for RISC-V 32-bit:

```bash
scripts/build-builtins.sh
```

This script:

1. Generates boilerplate code (registry, function references, etc.)
2. Compiles the application with aggressive optimizations
3. Outputs `target/riscv32imac-unknown-none-elf/release/lp-glsl-builtins-emu-app`

The build script automatically runs the code generator before building, so manual regeneration is
not required.

## Output

The compiled binary contains all builtin function symbols (`__lp_q32_*`) that are linked into GLSL
programs at runtime. The binary is statically linked and can be used as a library by the emulator.

## Target

Built for `riscv32imac-unknown-none-elf` with:

- Optimizations: `opt-level=1`
- Panic: `abort`
- No debug info
- Single codegen unit for better dead code elimination

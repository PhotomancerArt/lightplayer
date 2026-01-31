# lp-glsl-builtins-gen-app

Code generator that automatically generates boilerplate code for builtin functions by scanning the
`lp-glsl-builtins` crate.

## Overview

This tool eliminates manual maintenance of boilerplate code when adding new builtin functions. It
scans `lp-glsl/lp-glsl-builtins/src/q32/` for function definitions and generates:

- **registry.rs**: `BuiltinId` enum, `name()`, `signature()`, `all()`, and `get_function_pointer()`
  methods
- **builtin_refs.rs**: Function references to prevent dead code elimination in
  `lp-glsl-builtins-emu-app`
- **mod.rs**: Module declarations and `pub use` statements for all builtin functions
- **math.rs**: `map_testcase_to_builtin()` function mapping testcase names to `BuiltinId`

All generated files include clear headers indicating they are auto-generated and how to regenerate
them.

## Usage

Run the generator manually:

```bash
cd lp-glsl
cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl-builtins-gen-app/Cargo.toml
```

The generator is automatically invoked by `scripts/build-builtins.sh` before building, so manual
runs are typically not necessary.

## How It Works

1. Scans `crates/lp-glsl-builtins/src/q32/*.rs` (excluding `mod.rs` and `test_helpers.rs`)
2. Parses Rust source files using `syn` to find `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_*`
   declarations
3. Extracts function metadata: name, parameter count, symbol name
4. Generates enum variant names (e.g., `__lp_q32_sqrt` → `Q32Sqrt`)
5. Writes generated code to appropriate locations

## Adding New Builtins

When adding a new builtin function:

1. Create the function implementation in `crates/lp-glsl-builtins/src/q32/your_function.rs`
2. Run the build script: `scripts/build-builtins.sh`
3. The generator will automatically update all boilerplate files

No manual editing of registry, mod.rs, or other boilerplate files is required.


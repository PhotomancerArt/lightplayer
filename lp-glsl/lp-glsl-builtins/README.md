# lp-glsl-builtins

Low-level builtin function library for `lp-glsl-compiler`. Provides fixed-point arithmetic
functions, memory operations, and host interface functions that are linked into compiled GLSL
programs.

## Overview

This crate implements the core builtin functions used by compiled GLSL shaders. Functions operate on
fixed-point 16.16 format (i32 values with 16 bits of fractional precision) and are exported with
`#[no_mangle] pub extern "C"` for linking.

## Modules

- **q32**: Fixed-point 16.16 arithmetic functions (sin, cos, sqrt, exp, log, etc.)
- **mem**: Memory operations (memcpy, memset, memcmp) for no_std environments
- **host**: Host interface functions for debug output and system calls

## Usage

This crate is used as a dependency by other packages. It is not built standalone.

```toml
[dependencies]
lp-glsl-builtins = { path = "../../crates/lp-glsl-builtins" }
```

Functions are automatically registered via the generated registry in `lp-glsl-compiler`. To add a
new builtin function:

1. Create a new file in `src/q32/` with your function implementation
2. Run the generator to update boilerplate:
   `cargo run --bin lp-glsl-builtins-gen-app --manifest-path lightplayer/lp-glsl-builtins-gen-app/Cargo.toml`
3. Rebuild the builtins app: `scripts/build-builtins.sh`

## Building

The builtin functions are compiled into a static library via `lp-glsl-builtins-emu-app`. See that
package's README for build instructions.

//! Build script for lp-glsl-builtins
//!
//! This script is minimal - it just ensures the crate can be built as a staticlib.
//! The actual cross-compilation to riscv32imac-unknown-none-elf is handled by
//! lp-glsl-compiler's build.rs to avoid infinite loops.

fn main() {
    // This build.rs is intentionally minimal.
    // The actual building of lp-glsl-builtins for riscv32imac-unknown-none-elf
    // is done by lp-glsl-compiler's build.rs when needed.
}

//! Build script for lp-glsl-builtins
//!
//! This script is minimal — the crate builds as a staticlib for guest and host links.
//! RISC-V guest images and `lpir-cranelift` embedding use workspace scripts / `lpir-cranelift` build glue.

fn main() {
    // Intentionally empty — cross-target emu builds are driven by `lpir-cranelift` / workspace scripts.
}

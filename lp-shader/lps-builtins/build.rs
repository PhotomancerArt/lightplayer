//! Build script for lps-builtins
//!
//! This script is minimal — the crate builds as a staticlib for guest and host links.
//! RISC-V guest images and `lpvm-cranelift` embedding use workspace scripts / `lpvm-cranelift` build glue.

fn main() {
    // Intentionally empty — cross-target emu builds are driven by `lpvm-cranelift` / workspace scripts.
}

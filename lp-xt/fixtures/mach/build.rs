//! Link every `mach` fixture with xtensa-lx-rt's `link.x` (which pulls in the
//! generated `exception.x` laying the vector table out at `_init_start`) and
//! with this crate's `memory.x`, which names the five segments that script
//! expects plus `_stack_start_cpu0`.
//!
//! `link.x` and `exception.x` come from xtensa-lx-rt's own OUT_DIR, already on
//! the link search path via its `cargo:rustc-link-search`. `memory.x` is ours
//! and is copied into our OUT_DIR so `INCLUDE memory.x` resolves.

use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let memory_x = manifest.join("memory.x");

    fs::copy(&memory_x, out.join("memory.x")).expect("copy memory.x into OUT_DIR");
    println!("cargo:rustc-link-search={}", out.display());

    // No host startup files, and lx-rt's script rather than the fixtures'
    // `link.ld` (which has no vector segment and no `Reset`).
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-Tlink.x");

    println!("cargo:rerun-if-changed={}", memory_x.display());
}

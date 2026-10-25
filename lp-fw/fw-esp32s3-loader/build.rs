//! Write the loader's RAM into the linker script, from lp-bootctl.
//!
//! `loader.x` names two regions, `LIRAM` (code, instruction bus) and `LDRAM`
//! (data, data bus), and includes `loader_memory.x`, which this script
//! writes from [`lp_bootctl::S3_LOADER_RAM`]: the low half of that range
//! through its instruction-bus alias for code, the high half for data. The
//! numbers are the ones the core and `lp-fw-split` read, so they cannot
//! drift from the loader's own.
use std::{env, fs, path::PathBuf};

use lp_bootctl::{S3_LOADER_IRAM, S3_LOADER_RAM};

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let half = (S3_LOADER_RAM.end - S3_LOADER_RAM.start) / 2;
    let memory = format!(
        "/* Written by build.rs from lp_bootctl::S3_LOADER_RAM: do not edit. */\n\
         MEMORY {{\n  \
           LIRAM : ORIGIN = {:#010X}, LENGTH = {half:#X}\n  \
           LDRAM : ORIGIN = {:#010X}, LENGTH = {half:#X}\n\
         }}\n",
        S3_LOADER_IRAM.start,
        S3_LOADER_RAM.start + half,
    );
    fs::write(out.join("loader_memory.x"), memory).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg-bins=-Wl,-Tloader.x");
    println!("cargo:rerun-if-changed=loader.x");
    println!("cargo:rerun-if-changed=build.rs");
}

//! Build script for fw-esp32.
//!
//! Linker script (-Tlinkall.x) is configured via .cargo/config.toml to avoid
//! duplicate -Tlinkall.x (which would cause "region 'RAM' already defined").
//!
//! Patches esp-hal's eh_frame.x so .eh_frame is retained in ROM for unwinding.
//! esp-hal's default places .eh_frame at address 0 with (INFO) type = non-allocatable,
//! which discards unwind tables. We replace it with a no-op so our eh_frame_unwind.x
//! (loaded as a supplemental script) captures .eh_frame into ROM instead.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Patch esp-hal's linker scripts to retain .eh_frame inside .text.
    //
    // The ESP32 bootloader only supports 2 ROM-mapped segments (rodata + text).
    // .eh_frame must share the .text section to avoid creating a 3rd segment.
    // lld only merges content into one section when it's in the SAME definition,
    // so we patch text.x to include .eh_frame at the end of .text, and patch
    // eh_frame.x to a no-op (it would otherwise capture .eh_frame at address 0).
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let build_dir = out_dir.parent().unwrap().parent().unwrap();

    let patched_text = "\
SECTIONS {
  .text : ALIGN(4) {
    KEEP(*(.init));
    KEEP(*(.init.rust));
    KEEP(*(.text.abort));
    *(.literal .text .literal.* .text.*)
    /* Unwind tables: appended to .text so they share one ROM segment. */
    . = ALIGN(4);
    PROVIDE(__eh_frame = .);
    KEEP(*(.eh_frame));
    KEEP(*(.eh_frame.*));
  } > ROTEXT
}
";

    // Patch all esp-hal-* build dirs; Cargo may use any of them depending on feature set.
    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("esp-hal-") {
                let out_path = entry.path().join("out");
                if out_path.exists() {
                    let text_x = out_path.join("text.x");
                    if text_x.exists() {
                        let _ = std::fs::write(&text_x, patched_text);
                    }
                    let eh_frame_x = out_path.join("eh_frame.x");
                    if eh_frame_x.exists() {
                        let _ =
                            std::fs::write(&eh_frame_x, "/* patched: .eh_frame is in text.x */\n");
                    }
                }
            }
        }
    }

    let eh_frame = manifest_dir.join("linker").join("eh_frame_unwind.x");
    println!("cargo:rerun-if-changed={}", eh_frame.display());
    println!("cargo:rustc-link-arg=-T{}", eh_frame.display());
}

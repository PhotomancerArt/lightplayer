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
use std::process::Command;

/// Emit build provenance for the wire hello (`ServerHello.fw`):
/// `LP_BUILD_COMMIT` (short git commit or "unknown"), `LP_BUILD_DIRTY`
/// ("true"/"false", false when git is absent so vendored builds still
/// compile), and `LP_BUILD_PROFILE` (the cargo profile directory name,
/// e.g. "release-esp32", falling back to the coarse `PROFILE` env).
fn emit_build_provenance() {
    let commit =
        git_output(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = match git_output(&["status", "--porcelain"]) {
        Some(status) => !status.is_empty(),
        None => false,
    };
    let profile = profile_dir_name()
        .or_else(|| std::env::var("PROFILE").ok())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=LP_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=LP_BUILD_DIRTY={dirty}");
    println!("cargo:rustc-env=LP_BUILD_PROFILE={profile}");
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The actual profile directory name from OUT_DIR
/// (`…/<triple>/<profile>/build/<pkg>-<hash>/out`), which preserves custom
/// profile names like `release-esp32` that the `PROFILE` env collapses to
/// "release".
fn profile_dir_name() -> Option<String> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    // out -> <pkg>-<hash> -> build -> <profile>
    let profile = out_dir.parent()?.parent()?.parent()?;
    Some(profile.file_name()?.to_string_lossy().into_owned())
}

fn main() {
    emit_build_provenance();

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

    // The ESP32 bootloader only supports 2 ROM-mapped segments. espflash creates
    // image segments from ELF sections, splitting on gaps between sections. The
    // original rodata.x defines .rodata_desc and .rodata as separate output sections,
    // which creates a gap (due to .rodata's 128-byte input alignment) that espflash
    // treats as a segment boundary — producing 3 ROM segments and triggering
    // `rom_index < 2` in bootloader_utility.c. Fix: merge everything into one
    // .rodata output section so there's no gap.
    let patched_rodata = "\
SECTIONS {
  .rodata : ALIGN(4)
  {
    KEEP(*(.rodata_desc));
    KEEP(*(.rodata_desc.*));
    . = ALIGN(4);
    _rodata_start = ABSOLUTE(.);
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
    *(.gcc_except_table .gcc_except_table.*)
    . = ALIGN(4);
    *( .rodata_wlog_*.* )
    . = ALIGN(4);
    _rodata_end = ABSOLUTE(.);
  } > RODATA
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
                        std::fs::write(&text_x, patched_text).unwrap_or_else(|e| {
                            panic!("failed to patch {}: {e}", text_x.display())
                        });
                    }
                    let eh_frame_x = out_path.join("eh_frame.x");
                    if eh_frame_x.exists() {
                        std::fs::write(&eh_frame_x, "/* patched: .eh_frame is in text.x */\n")
                            .unwrap_or_else(|e| {
                                panic!("failed to patch {}: {e}", eh_frame_x.display())
                            });
                    }
                    let rodata_x = out_path.join("rodata.x");
                    if rodata_x.exists() {
                        std::fs::write(&rodata_x, patched_rodata).unwrap_or_else(|e| {
                            panic!("failed to patch {}: {e}", rodata_x.display())
                        });
                    }
                }
            }
        }
    }

    // No cargo:rerun-if-changed restriction — we must re-run whenever esp-hal's
    // build hash changes so we can patch its freshly-generated text.x / eh_frame.x.
    // Cargo's default (re-run when any package file changes) is the right behavior.
    let eh_frame = manifest_dir.join("linker").join("eh_frame_unwind.x");
    println!("cargo:rustc-link-arg=-T{}", eh_frame.display());
}

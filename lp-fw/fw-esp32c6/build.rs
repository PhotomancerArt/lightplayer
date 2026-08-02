//! Build script for fw-esp32c6.
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

/// Overwrite `path` (if it exists) with `contents`, then backdate its mtime
/// to the epoch. The backdating matters: this script watches these files via
/// rerun-if-changed, and cargo's staleness reference is the script's *start*
/// time — so our own writes would otherwise look newer and self-invalidate
/// the script, re-running it (and rebuilding fw-esp32c6) on every build. With
/// the epoch mtime, only a regeneration by esp-hal's build script (which
/// writes with a real timestamp) registers as a change.
fn patch_file(path: &std::path::Path, contents: &str) {
    if !path.exists() {
        return;
    }
    std::fs::write(path, contents)
        .unwrap_or_else(|e| panic!("failed to patch {}: {e}", path.display()));
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(std::time::SystemTime::UNIX_EPOCH))
        .unwrap_or_else(|e| panic!("failed to backdate {}: {e}", path.display()));
}

fn main() {
    emit_build_provenance();
    emit_partition_facts();

    // Harness builds: any test_* feature except test_oom selects a hardware
    // harness entrypoint instead of the app (test_oom runs the full app plus
    // an OOM/panic exercise). Collapsed to one cfg so app-only code carries a
    // single gate instead of a 12-feature wall at every site.
    println!("cargo::rustc-check-cfg=cfg(fw_harness)");
    let harness = std::env::vars()
        .any(|(k, _)| k.starts_with("CARGO_FEATURE_TEST_") && k != "CARGO_FEATURE_TEST_OOM");
    if harness {
        println!("cargo::rustc-cfg=fw_harness");
    }

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
    let mut found_esp_hal_out = false;
    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("esp-hal-") {
                let out_path = entry.path().join("out");
                if out_path.exists() {
                    found_esp_hal_out = true;
                    // Watch the files we patch: if esp-hal's build script
                    // re-runs it regenerates them pristine, and (with no
                    // `links` key on esp-hal) cargo gives no ordering edge
                    // between its script and ours — so the fresh mtimes must
                    // dirty this script for the next build to re-patch.
                    for file in ["text.x", "eh_frame.x", "rodata.x"] {
                        println!("cargo:rerun-if-changed={}", out_path.join(file).display());
                    }
                    patch_file(&out_path.join("text.x"), patched_text);
                    patch_file(
                        &out_path.join("eh_frame.x"),
                        "/* patched: .eh_frame is in text.x */\n",
                    );
                    patch_file(&out_path.join("rodata.x"), patched_rodata);
                }
            }
        }
    }

    // Emitting rerun-if-changed disables cargo's default rule (re-run when any
    // package file changes), so restate it as the package dir. Together with
    // the esp-hal watches above this is the staleness guard: if esp-hal's
    // script re-runs while ours stays fingerprint-fresh, the regenerated
    // files dirty this script and the next build re-patches. A same-build
    // regeneration can still slip one failing link through (no ordering
    // guarantee between the two scripts), surfacing as `undefined symbol:
    // __eh_frame` — pristine text.x always kills the link, so that error is
    // the tripwire for the whole stale set, and one rebuild self-heals it.
    println!("cargo:rerun-if-changed={}", manifest_dir.display());
    if !found_esp_hal_out {
        // esp-hal's out dir doesn't exist yet (its script hasn't run).
        // Watching a path that never exists forces a re-run every build
        // until the dir appears and gets patched.
        println!(
            "cargo:rerun-if-changed={}",
            build_dir.join("esp-hal-out-pending").display()
        );
    }

    let eh_frame = manifest_dir.join("linker").join("eh_frame_unwind.x");
    println!("cargo:rustc-link-arg=-T{}", eh_frame.display());
}

/// Emit `LP_FLASH_APP_BYTES` from partitions.csv's `app` row, so the embedded
/// firmware manifest's limits come from the same file espflash flashes with —
/// never a hand-transcribed integer (cf.
/// docs/debt/firmware-partition-constants-transcribed.md).
fn emit_partition_facts() {
    let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("partitions.csv");
    let csv = std::fs::read_to_string(&path).expect("read partitions.csv");
    let size_field = csv
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(|line| line.split(',').map(str::trim).collect::<Vec<_>>())
        .find(|fields| fields.len() >= 5 && fields[1] == "app")
        .map(|fields| fields[4].to_string())
        .expect("partitions.csv has an app row");
    println!(
        "cargo:rustc-env=LP_FLASH_APP_BYTES={}",
        parse_partition_size(&size_field)
    );
}

fn parse_partition_size(s: &str) -> u64 {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).expect("hex partition size")
    } else if let Some(mega) = s.strip_suffix(['M', 'm']) {
        mega.parse::<u64>().expect("M partition size") * 1024 * 1024
    } else if let Some(kilo) = s.strip_suffix(['K', 'k']) {
        kilo.parse::<u64>().expect("K partition size") * 1024
    } else {
        s.parse().expect("partition size")
    }
}

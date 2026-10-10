//! Build script for fw-esp32s3.
//!
//! Deliberately minimal. The C6 counterpart patches esp-hal's `eh_frame.x` so
//! `.eh_frame` survives into ROM for `unwinding`-based panic recovery; this
//! chip uses **abort-tier** recovery (ADR 2026-07-29-per-chip-fw-toolchains),
//! so there are no unwind tables to preserve and none of that machinery
//! belongs here.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Watch the whole package, not just this file. Emitting ANY
    // rerun-if-changed disables cargo's default "re-run when any package file
    // changes" rule, so naming only `build.rs` pins the provenance stamp below
    // to whatever commit was checked out the first time this script ran — the
    // device then reports a stale commit in its wire hello, which is exactly
    // the fact you reach for when asking "which build is on this board?".
    // Observed during M3's hardware walk: the board reported P5's commit while
    // running a P6 image. Same shape as fw-esp32c6's build.rs, which restates
    // the package dir for the same reason.
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );

    // Put this crate's directory on the linker search path so esp-hal's
    // `INCLUDE "rwdata_hook.x"` resolves to `rwdata_hook.x` next to this file.
    // esp-hal's `ld/sections/rwdata.x` ends the `.data` output section with
    // that INCLUDE, gated on `ESP_HAL_CONFIG_USE_RWDATA_LD_HOOK` (set in
    // `.cargo/config.toml`); the linker resolves it through `-L`, and esp-hal
    // only adds its own OUT_DIR. See `rwdata_hook.x` for what stays in RAM.
    println!(
        "cargo:rustc-link-search={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );

    emit_build_provenance();
    emit_partition_facts();

    // Harness builds: any `test_*` feature selects a hardware harness
    // entrypoint instead of the app. Collapsed to a single cfg so app-only
    // code carries one gate rather than a wall of per-feature conditions —
    // the same shape fw-esp32c6 uses, deliberately not a second mechanism.
    //
    // The `LP_FP_*` rerun-if-env-changed lines that used to live here moved to
    // `lp-xt-fp-harness/build.rs` along with the `option_env!` calls that read
    // them. That tracking has to sit in the crate where the macro expands, not
    // in the crate that turns the feature on.
    println!("cargo::rustc-check-cfg=cfg(fw_harness)");
    let harness = std::env::vars().any(|(k, _)| k.starts_with("CARGO_FEATURE_TEST_"));
    if harness {
        println!("cargo::rustc-cfg=fw_harness");
    }
}

/// Emit build provenance for the wire hello (`ServerHello.fw`):
/// `LP_BUILD_COMMIT` (short git commit or "unknown"), `LP_BUILD_DIRTY`
/// ("true"/"false", false when git is absent so vendored builds still
/// compile), and `LP_BUILD_PROFILE` (the cargo profile directory name).
///
/// Same three variables as fw-esp32c6's build script, for the same reason: the
/// server is sans-IO and never reads git or env itself, so the binary has to
/// bake them in. `main.rs` injects them into `LpServer::set_hello`.
///
/// Plus `LP_APP_VERSION`, the build's app version, from the one helper every
/// versioned build uses (`tools/lp-app-version`) — never computed here.
fn emit_build_provenance() {
    lp_app_version::emit();
    lp_app_version::emit_target();
    emit_git_head_watches();
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
    println!("cargo:rustc-env=LP_BUILD_FEATURES={}", enabled_features());
}

/// This crate's enabled cargo features, comma-separated and sorted, for the
/// `[fw-checks-header]` line the payload harnesses print
/// (`fw_checks::PayloadHeader::firmware_features`, M6 P08). The point is that
/// a transcript states the image's feature set without anyone retyping it;
/// sorted, so two builds of one feature set produce one string.
///
/// ⚠️ **The spelling has to be recovered, not guessed.** Cargo uppercases a
/// feature name and turns `-` into `_` to build `CARGO_FEATURE_*`, and that
/// map is not invertible: `fw-esp32c6`'s build script lowercases and leaves
/// `_` alone, which is exact only because every feature that crate declares
/// uses `_`. This one declares `float-f32`, `frame-dump`, `fixture-old-proto`
/// and `fixture-no-hello`, so the same trick would write `float_f32` into a
/// header — a feature name that does not exist and that no `cargo build`
/// would accept. So the declared names come out of this crate's own
/// `Cargo.toml`, which is the only place they are spelled correctly. Ported
/// from `fw-esp32v3/build.rs`, which hit this first.
fn enabled_features() -> String {
    let declared = declared_features();
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_string))
        .map(|var| {
            declared
                .iter()
                .find(|name| cargo_feature_var(name) == var)
                .cloned()
                // A feature cargo told us about that the manifest does not
                // declare cannot happen; fall back rather than panic, so a
                // future cargo change degrades a header instead of a build.
                .unwrap_or_else(|| var.to_ascii_lowercase())
        })
        .collect();
    features.sort();
    features.join(",")
}

/// `CARGO_FEATURE_*`'s suffix for a declared feature name, by cargo's rule.
fn cargo_feature_var(name: &str) -> String {
    name.to_ascii_uppercase().replace('-', "_")
}

/// The feature names declared in this crate's `[features]` table, read from
/// the manifest so the spelling is the manifest's.
fn declared_features() -> Vec<String> {
    let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).expect("read Cargo.toml");
    let mut names = Vec::new();
    let mut in_features = false;
    for line in manifest.lines() {
        let trimmed = line.trim_start();
        // Section headers are at column zero in this manifest; a `[` that
        // starts a trimmed line inside `[features]` is an array value's, so
        // only an untrimmed-column-zero `[` ends the table.
        if line.starts_with('[') {
            in_features = line.trim_end() == "[features]";
            continue;
        }
        if !in_features || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            names.push(name.to_string());
        }
    }
    names
}

/// Watch git's HEAD so this script reruns when the checked-out commit moves —
/// a commit, checkout, or rebase — even though nothing under the package
/// directory changed. Without this, `LP_BUILD_COMMIT` above only gets
/// re-read on an incremental build that also touched a file cargo was
/// already watching, so a local build can keep reporting a stale commit in
/// the manifest and the wire hello. Same helper as fw-esp32c6's build
/// script; see there for the full reasoning.
///
/// Three paths, chosen to work in both a primary checkout and a worktree
/// (where `.git` is a file, not a directory):
/// - `<git-dir>/HEAD` — this checkout's own HEAD, present whether attached
///   to a branch or detached. `git rev-parse --git-dir` already resolves
///   the worktree indirection to the right place.
/// - the ref HEAD points to, under the *common* dir (`git rev-parse
///   --git-common-dir`) — branches are shared across worktrees, so a commit
///   made from any of them updates the loose ref file there. Found via
///   `git rev-parse --symbolic-full-name HEAD`; skipped on a detached HEAD,
///   where that command's output is not a `refs/...` path and the HEAD file
///   alone is enough.
/// - `<common-dir>/packed-refs`, if present — covers a branch that is only
///   packed (e.g. right after a fresh clone), where committing may update
///   this file rather than create a loose ref.
///
/// If git is unavailable (e.g. a source tarball with no `.git`), this emits
/// nothing extra; `LP_BUILD_COMMIT` already falls back to "unknown" in that
/// case regardless.
fn emit_git_head_watches() {
    let Some(git_dir) = git_path("--git-dir") else {
        return;
    };
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());

    let Some(common_dir) = git_path("--git-common-dir") else {
        return;
    };
    if let Some(symbolic) = git_output(&["rev-parse", "--symbolic-full-name", "HEAD"]) {
        if symbolic.starts_with("refs/") {
            println!(
                "cargo:rerun-if-changed={}",
                common_dir.join(symbolic).display()
            );
        }
    }
    let packed_refs = common_dir.join("packed-refs");
    if packed_refs.exists() {
        println!("cargo:rerun-if-changed={}", packed_refs.display());
    }
}

/// `git rev-parse <arg>` as a path, made absolute against
/// `CARGO_MANIFEST_DIR` when git prints a relative one (cargo resolves a
/// relative `rerun-if-changed` against the package root, which is this
/// script's own current directory, but an explicit join avoids relying on
/// that coincidence).
fn git_path(arg: &str) -> Option<PathBuf> {
    let raw = git_output(&["rev-parse", arg])?;
    let path = PathBuf::from(raw);
    Some(if path.is_absolute() {
        path
    } else {
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join(path)
    })
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
/// profile names that the coarse `PROFILE` env collapses to "release".
fn profile_dir_name() -> Option<String> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    // out -> <pkg>-<hash> -> build -> <profile>
    let profile = out_dir.parent()?.parent()?.parent()?;
    Some(profile.file_name()?.to_string_lossy().into_owned())
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

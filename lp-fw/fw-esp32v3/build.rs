//! Build script for fw-esp32v3.
//!
//! Deliberately minimal, mirroring fw-esp32s3's: this chip is also abort-tier
//! (ADR 2026-07-29-per-chip-fw-toolchains), so there is no `.eh_frame`
//! patching to do — that machinery is the C6's `panic=unwind` tier only.
//!
//! The `fw_harness` half mirrors fw-esp32s3's: any `test_*` feature selects a
//! hardware harness entrypoint instead of the app, collapsed to a single cfg so
//! app-only code carries one gate rather than a wall of per-feature conditions.
//!
//! The `LP_FP_*` rerun-if-env-changed tracking that fw-esp32s3 used to carry is
//! **not** duplicated here. It lives in `lp-xt-fp-harness/build.rs`, next to the
//! `option_env!` calls that read those variables — tracking has to sit where the
//! macro expands, not where the feature is switched on.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Watch the whole package, not just this file. Emitting ANY
    // rerun-if-changed disables cargo's default "re-run when any package file
    // changes" rule, so naming only `build.rs` would pin the provenance stamp
    // below to whatever commit was checked out the first time this script ran
    // — the device then reports a stale commit in its wire hello, which is
    // exactly the fact you reach for when asking "which build is on this
    // board?". fw-esp32s3 observed precisely that during its M3 hardware walk.
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );

    emit_build_provenance();
    emit_partition_facts();
    emit_linker_search_path();
    emit_bench_project();

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
/// Same three variables as fw-esp32s3's and fw-esp32c6's build scripts, for
/// the same reason: the server is sans-IO and never reads git or env itself,
/// so the binary has to bake them in. `main.rs` injects them into
/// `LpServer::set_hello`.
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
    println!("cargo:rustc-env=LP_BUILD_FEATURES={}", enabled_features());
}

/// This crate's enabled cargo features, comma-separated and sorted, for the
/// `[fw-checks-header]` line the M5 P2 payload harnesses print
/// (`fw_checks::PayloadHeader::firmware_features`). The point is that a
/// transcript states the image's feature set without anyone retyping it;
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
/// `Cargo.toml`, which is the only place they are spelled correctly.
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

/// Put this crate's directory on the linker search path so esp-hal's
/// `INCLUDE "rwdata_hook.x"` resolves to `rwdata_hook.x` next to this file.
///
/// esp-hal's `ld/sections/rwdata.x` ends the `.data` output section with that
/// INCLUDE, gated on `ESP_HAL_CONFIG_USE_RWDATA_LD_HOOK` (set in
/// `.cargo/config.toml`). The INCLUDE is resolved by the linker's `-L` path,
/// and esp-hal only adds its own OUT_DIR — a firmware that wants to supply the
/// hook has to add its own directory, which is all this does.
///
/// See `rwdata_hook.x` for what the hook keeps in RAM and why.
fn emit_linker_search_path() {
    println!(
        "cargo:rustc-link-search={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );
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

/// Stage the `bench_render_loop` image's project, **retargeted to a pad this
/// board has**, into `OUT_DIR` for `src/bench/render_loop.rs` to
/// `include_bytes!`.
///
/// # Why a rewrite and not a second copy of the project
///
/// The benchmark's whole point is that the classic and the C6 render the same
/// pixels, so the ratio between the two machines is a ratio and not a
/// comparison of two workloads. `projects/test/basic` is the C6's image
/// (`lp-fw/fw-esp32c6/src/bench/render_loop.rs`), and its output addresses
/// `ws281x:local:D10` — the XIAO's silkscreen for the very same gpio18 the
/// DOM-Z-102 calls `IO18`. The classic's endpoint table is built from
/// `display_label` alone (`esp32v3_rmt_ws281x_driver`), so `D10` resolves to
/// nothing here and the image would render in silence.
///
/// So the bytes are rewritten at build time, exactly as
/// `scripts/m4-hardware-walk.sh`'s `prepare_project` and
/// `scripts/emu/m4-walk-esp32v3.sh` rewrite them before an upload: one plain
/// substitution, in `output.json` alone, `"ws281x:local:D10"` ->
/// `"ws281x:local:IO18"`. A **forked copy** of the project under some
/// classic-only path would be the other option and it is the wrong one: two
/// files that must stay identical except for one label is a drift waiting to
/// happen, and the first person to edit the shader on one side would make the
/// two chips' numbers incomparable without anything failing.
///
/// The substitution count is asserted, mirroring the scripts' own `grep -q`
/// guard: a project that stopped naming `D10` must fail the build rather than
/// bake an endpoint this board does not have.
fn emit_bench_project() {
    if std::env::var_os("CARGO_FEATURE_BENCH_RENDER_LOOP").is_none() {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("../../projects/test/basic");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("bench-project");
    std::fs::create_dir_all(&out).expect("create OUT_DIR/bench-project");

    // The source project is outside CARGO_MANIFEST_DIR, so the package-wide
    // `rerun-if-changed` above does not cover it.
    println!("cargo:rerun-if-changed={}", src.display());

    for name in BENCH_PROJECT_FILES {
        let from = src.join(name);
        let bytes =
            std::fs::read(&from).unwrap_or_else(|e| panic!("reading {}: {e}", from.display()));
        let bytes = if *name == "output.json" {
            let text = String::from_utf8(bytes).expect("output.json is UTF-8");
            let hits = text.matches(BENCH_PAD_FROM).count();
            assert_eq!(
                hits,
                1,
                "{} names {BENCH_PAD_FROM} {hits} times, expected exactly 1 — the classic's \
                 bench image retargets it to {BENCH_PAD_TO} and cannot guess what to do with \
                 none or several",
                from.display()
            );
            text.replace(BENCH_PAD_FROM, BENCH_PAD_TO).into_bytes()
        } else {
            bytes
        };
        std::fs::write(out.join(name), bytes)
            .unwrap_or_else(|e| panic!("writing {}: {e}", out.join(name).display()));
    }
}

/// The eight files of `projects/test/basic`, in the order the C6's bench seeds
/// them. Named rather than globbed: a project that grew a ninth file should
/// fail the seed loudly in review, not silently change what is measured.
const BENCH_PROJECT_FILES: &[&str] = &[
    "project.json",
    "module.json",
    "clock.json",
    "shader.json",
    "shader.glsl",
    "fixture.json",
    "fixture.map2d.json",
    "output.json",
];

/// The XIAO ESP32-C6's silkscreen for gpio18 …
const BENCH_PAD_FROM: &str = "ws281x:local:D10";
/// … and the DOM-Z-102's, for the same physical pin.
const BENCH_PAD_TO: &str = "ws281x:local:IO18";

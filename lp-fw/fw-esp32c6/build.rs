//! Build script for fw-esp32c6.
//!
//! Linker script (-Tlinkall.x) is configured via .cargo/config.toml to avoid
//! duplicate -Tlinkall.x (which would cause "region 'RAM' already defined").
//!
//! Patches two of esp-hal's generated linker scripts, both because the ESP32
//! bootloader maps at most 2 ROM segments:
//!
//! - `rodata.x`, to merge `.rodata_desc` and `.rodata` into ONE output section.
//!   esp-hal's default defines them separately, and `.rodata`'s 128-byte input
//!   alignment leaves a gap that espflash reads as a segment boundary —
//!   producing 3 ROM-mapped segments and tripping the ESP32 bootloader's
//!   `rom_index < 2` assert. See `89487cc05`.
//! - `eh_frame.x`, flattened to a no-op so nothing captures `.eh_frame` into a
//!   section of its own.
//!
//! Until 2026-08-02 it also patched `text.x`, to capture `.eh_frame` into
//! `.text` for the `unwinding` crate. That is gone with the rest of the unwind
//! tier (ADR `2026-08-02-rv32-firmwares-are-abort-tier`), and with it the
//! `text.x` patch — see the comment in `main` for why it was not merely
//! trimmed.

use std::path::PathBuf;
use std::process::Command;

/// Emit build provenance for the wire hello (`ServerHello.fw`):
/// `LP_BUILD_COMMIT` (short git commit or "unknown"), `LP_BUILD_DIRTY`
/// ("true"/"false", false when git is absent so vendored builds still
/// compile), `LP_BUILD_PROFILE` (the cargo profile directory name,
/// e.g. "release-esp32", falling back to the coarse `PROFILE` env), and
/// `LP_BUILD_FEATURES` (this crate's enabled cargo features, comma-separated
/// and sorted) for the validation system's transcript header.
///
/// Plus `LP_APP_VERSION`, the build's app version, and `LP_FW_TARGET`, the
/// target (build def id) it was built as, from the one helper every
/// versioned build uses (`tools/lp-app-version`) — never computed here.
fn emit_build_provenance() {
    lp_app_version::emit();
    lp_app_version::emit_target();
    emit_git_head_watches();
    let commit =
        git_output(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into());
    // A figure build is stamped clean, as it is stamped with a fixed version
    // (`lp_app_version::figure_build`): the tree's state must not reach the
    // image whose memory figures are recorded.
    let dirty = !lp_app_version::figure_build()
        && match git_output(&["status", "--porcelain"]) {
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

/// This crate's enabled cargo features, from cargo's own `CARGO_FEATURE_*`
/// environment.
///
/// Cargo uppercases a feature name and turns `-` into `_` to build those
/// variable names, so `check-json` arrives as `CARGO_FEATURE_CHECK_JSON`. The
/// inverse is not unique — this lowercases and leaves `_` alone, which is
/// exact for every feature this crate declares (they all use `_`, never `-`).
///
/// The point is that a transcript's header should state the image's feature set
/// without anyone retyping it. Sorted so two builds of the same feature set
/// produce the same string.
fn enabled_features() -> String {
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| {
            key.strip_prefix("CARGO_FEATURE_")
                .map(str::to_ascii_lowercase)
        })
        .collect();
    features.sort();
    features.join(",")
}

/// Watch git's HEAD so this script reruns when the checked-out commit moves —
/// a commit, checkout, or rebase — even though nothing under the package
/// directory changed. Without this, `LP_BUILD_COMMIT` above only gets
/// re-read on an incremental build that also touched a file cargo was
/// already watching, so a local build can keep reporting a stale commit in
/// the manifest and the wire hello.
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
    // Absent is a hard error, never a skip. A skipped patch links esp-hal's
    // stock layout, which produces a THREE-ROM-segment image that builds
    // clean and then dies in the bootloader — see the `rom_index < 2` note
    // in `main`. There is no situation in which carrying on is better than
    // saying so here.
    assert!(
        path.exists(),
        "esp-hal generated no {}, so there is nothing to patch. \
         DEP_ESP_HAL_LINKER_SCRIPTS pointed at {}; if esp-hal stopped \
         generating this script, this patch needs rewriting against the new \
         layout rather than skipping.",
        path.file_name().unwrap_or_default().to_string_lossy(),
        path.parent().unwrap_or(path).display(),
    );
    std::fs::write(path, contents)
        .unwrap_or_else(|e| panic!("failed to patch {}: {e}", path.display()));
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(std::time::SystemTime::UNIX_EPOCH))
        .unwrap_or_else(|e| panic!("failed to backdate {}: {e}", path.display()));
}

/// The cloud relay's device-leg address (Wi-Fi relay plan RD14): the
/// product image always dials `lightplayer.app:80`; a **desk** image built
/// with `LP_RELAY_HOST=<host>[:port]` dials that instead (a local
/// `lp-cloud-server`), and says so in its boot banner. A malformed value
/// fails the build rather than ship an image that dials nowhere.
fn emit_relay_host() {
    println!("cargo:rerun-if-env-changed=LP_RELAY_HOST");
    let given = std::env::var("LP_RELAY_HOST")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let (host, port, overridden) = match &given {
        None => ("lightplayer.app".to_string(), 80u16, false),
        Some(value) => {
            let value = value.trim();
            match value.rsplit_once(':') {
                Some((host, port)) => {
                    let port: u16 = port.parse().unwrap_or_else(|_| {
                        panic!("LP_RELAY_HOST={value}: '{port}' is not a port")
                    });
                    (host.to_string(), port, true)
                }
                None => (value.to_string(), 80, true),
            }
        }
    };
    assert!(
        !host.is_empty() && !host.contains('/'),
        "LP_RELAY_HOST takes host[:port], not a URL"
    );
    println!("cargo:rustc-env=LP_RELAY_HOST_NAME={host}");
    println!("cargo:rustc-env=LP_RELAY_HOST_PORT={port}");
    println!("cargo:rustc-env=LP_RELAY_HOST_OVERRIDDEN={overridden}");
}

fn main() {
    emit_build_provenance();
    emit_partition_facts();
    emit_relay_host();

    // Harness builds: any test_* feature selects a hardware harness entrypoint
    // instead of the app. Collapsed to one cfg so app-only code carries a
    // single gate instead of a 12-feature wall at every site.
    //
    // `test_oom` used to be the one exception — it ran the full app plus an
    // OOM/panic exercise rather than replacing the entrypoint — and it went
    // with the unwind tier it existed to validate.
    println!("cargo::rustc-check-cfg=cfg(fw_harness)");
    // Split-link builds (the split pipeline sets `LP_SPLIT_LINK`): the core
    // reaches the engine only through its header, and the boot-record path
    // is compiled in. An env var rather than a feature: both link passes
    // must see the same code, and a feature would change every crate's
    // fingerprint.
    println!("cargo::rustc-check-cfg=cfg(lp_split)");
    println!("cargo:rerun-if-env-changed=LP_SPLIT_LINK");
    if std::env::var_os("LP_SPLIT_LINK").is_some() {
        println!("cargo::rustc-cfg=lp_split");
    }
    let harness = std::env::vars().any(|(k, _)| k.starts_with("CARGO_FEATURE_TEST_"));
    if harness {
        println!("cargo::rustc-cfg=fw_harness");
    }
    // The network on `lp-net` (`src/net/`): the `wifi` feature in an app
    // image whose radio is the product's. Stress and desk-meter builds give
    // the radio to their load generators instead, and harnesses replace the
    // app, so none of them run it. One cfg, so each site carries one gate.
    println!("cargo::rustc-check-cfg=cfg(lp_net)");
    let feature = |name: &str| std::env::var_os(format!("CARGO_FEATURE_{name}")).is_some();
    if feature("WIFI")
        && !harness
        && !feature("STRESS_S2")
        && !feature("STRESS_S3")
        && !feature("DESK_ESPNOW_METER")
    {
        println!("cargo::rustc-cfg=lp_net");
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Patch esp-hal's generated linker scripts.
    //
    // The ESP32 bootloader only supports 2 ROM-mapped segments (rodata + text),
    // and that single constraint is why both patches below exist.
    //
    // `text.x` is NOT patched any more. It used to be, to append `.eh_frame`
    // inside the `.text` output section — lld only merges input sections into
    // one output section when they appear in the same `SECTIONS { .text : {} }`
    // block, so a separately-defined `.eh_frame` always became a third ROM
    // segment. With the unwind tier gone (ADR
    // `2026-08-02-rv32-firmwares-are-abort-tier`) there is nothing to append,
    // and what remained of our patch was byte-identical to esp-hal's own
    // `ld/sections/text.x` for riscv. Patching a file to its own contents is
    // just a way to break when upstream changes it.
    //
    // `eh_frame.x` is still flattened to a no-op even though this image no
    // longer unwinds: esp-hal's version captures `.eh_frame` into its own
    // output section, which would be that third ROM segment if anything ever
    // emitted one. With the no-op and no KEEP in `text.x`, `.eh_frame` has no
    // home and is dropped entirely — verified: the linked ELF has no
    // `.eh_frame` section and no `__eh_frame` symbol.

    // WHERE esp-hal's generated linker scripts are, told to us by esp-hal
    // itself rather than guessed.
    //
    // This used to scan `target/<triple>/<profile>/build/` for an `esp-hal-*`
    // directory with an `out/` in it, and skip quietly when it found none.
    // The skip was the bug (docs/defects/2026-09-08-cold-target-dir-
    // links-esp-hals-stock-rodata.md): cargo runs build scripts concurrently unless
    // something orders them, and esp-hal declared no `links` key, so on a
    // COLD target dir this script could run before esp-hal's, find no out
    // dir, patch nothing, and link the stock layout. Every later build in
    // that tree re-patched — so build 1 and build 2 of one commit were
    // different images, and a CI tree is always cold.
    //
    // The LP esp-hal fork now carries `links = "esp-hal"` and emits
    // `cargo::metadata=linker-scripts=$OUT_DIR` (third_party/esp-hal,
    // README-LP.md "The second diff"). The `links` key is what buys both
    // halves of the fix: cargo runs esp-hal's build script first, and its
    // metadata arrives here as `DEP_ESP_HAL_LINKER_SCRIPTS`. Cargo also makes
    // this script's fingerprint depend on esp-hal's, so a rerun of esp-hal's
    // script (which regenerates the scripts pristine) re-runs this one after
    // it — closing the same-build half of the race that the `rerun-if-changed`
    // watches below could only catch on the NEXT build.
    let esp_hal_ld = PathBuf::from(std::env::var("DEP_ESP_HAL_LINKER_SCRIPTS").unwrap_or_else(
        |_| {
            panic!(
                "DEP_ESP_HAL_LINKER_SCRIPTS is unset: esp-hal did not publish where it \
                 generated its linker scripts, so the two patches below cannot be applied.\n\
                 \n\
                 That variable comes from `links = \"esp-hal\"` plus \
                 `cargo::metadata=linker-scripts=…` in third_party/esp-hal (see its \
                 README-LP.md). If the fork was dropped for an upstream esp-hal release, \
                 those two lines have to move with it — otherwise cargo orders nothing \
                 between esp-hal's build script and this one, and a cold target dir links \
                 esp-hal's stock rodata.x. That image builds clean and then asserts in the \
                 bootloader (`unpack_load_app, bootloader_utility.c:762, rom_index < 2`).\n\
                 \n\
                 Failing the build here is deliberate. See \
                 docs/defects/2026-09-08-cold-target-dir-links-esp-hals-stock-rodata.md."
            )
        },
    ));

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
    . = ALIGN(4);
    *( .rodata_wlog_*.* )
    . = ALIGN(4);
    _rodata_end = ABSOLUTE(.);
  } > RODATA
}
";

    // Watch the files we patch: if esp-hal's build script re-runs it
    // regenerates them pristine, and the fresh mtimes must dirty this script
    // so the patch is re-applied. (With the `links` edge above, cargo's own
    // fingerprint propagation already does this; the watches are the belt to
    // that braces, and they cost nothing.)
    for file in ["eh_frame.x", "rodata.x"] {
        println!("cargo:rerun-if-changed={}", esp_hal_ld.join(file).display());
    }
    patch_file(
        &esp_hal_ld.join("eh_frame.x"),
        "/* patched: this image is abort tier and emits no unwind tables */\n",
    );
    patch_file(&esp_hal_ld.join("rodata.x"), patched_rodata);

    // Emitting rerun-if-changed disables cargo's default rule (re-run when any
    // package file changes), so restate it as the package dir.
    //
    // ⚠️ If this patch ever stops being applied, the failure is SILENT AT
    // BUILD TIME and it did not use to be. While `text.x` carried our
    // `__eh_frame` symbol, a pristine copy always killed the link with
    // `undefined symbol: __eh_frame`, which was the tripwire for the whole
    // stale set. With the unwind tier gone there is no such reference: a
    // pristine `rodata.x` links fine and produces THREE ROM-mapped segments,
    // and the board then fails at boot inside the bootloader (`Assert failed
    // in unpack_load_app, bootloader_utility.c:762 (rom_index < 2)`). That is
    // why the two failure modes this script CAN see — no
    // `DEP_ESP_HAL_LINKER_SCRIPTS`, or a missing script inside it — both abort
    // the build instead of carrying on.
    println!("cargo:rerun-if-changed={}", manifest_dir.display());

    // Put this crate's directory on the linker search path so esp-hal's
    // `INCLUDE "rwdata_hook.x"` resolves to `rwdata_hook.x` next to this file.
    // esp-hal's `ld/sections/rwdata.x` ends the `.data` output section with
    // that INCLUDE, gated on `ESP_HAL_CONFIG_USE_RWDATA_LD_HOOK` (set in
    // `.cargo/config.toml`); the linker resolves it through `-L`, and esp-hal
    // only adds its own OUT_DIR. See `rwdata_hook.x` for what stays in RAM.
    println!("cargo:rustc-link-search={}", manifest_dir.display());
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

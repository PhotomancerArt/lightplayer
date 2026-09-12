//! The one test in this crate that is **never** compiled out, and whose only
//! job is to say what is.
//!
//! `tests/translate_roundtrip.rs` and `tests/guard_traps.rs` — the suites that
//! run emitted modules in a real engine and compare what came back — are
//! `#![cfg(feature = "host-wasmtime")]` at file scope. Without the feature
//! they compile to nothing at all: `cargo test -p lp-emu-jit` reports two test
//! binaries with `0 tests` and exits 0, and a reader has no way to tell that
//! from "the engine agrees". CI's `validate-x64` runs `cargo test --workspace`
//! at default features, so that is exactly what CI has been reporting.
//!
//! M7 JD14 says neither oracle tier may skip silently. This is the default
//! path's half of that: a named test that prints what is missing and how to
//! run it. It does not fail — the feature is off by default on purpose
//! (wasmtime is 130–220 s of cranelift and no other job should pay for it,
//! JD18) — but nobody can now read a green `cargo test -p lp-emu-jit` as
//! evidence that a module ran anywhere.
//!
//! It also guards against its own drift: the file list below is checked
//! against the directory, so a new engine suite that forgets to be named here,
//! or a rename of one that is, fails this test rather than quietly widening
//! the hole.

use std::path::{Path, PathBuf};

/// The suites that only exist when `host-wasmtime` is on, and one line each
/// saying what stops being checked without it.
const ENGINE_SUITES: &[(&str, &str)] = &[
    (
        "translate_roundtrip.rs",
        "every emitted module run under wasmtime and compared field by field — \
         registers, both counters, the exit pc, the escape-hatch build against \
         the emitted one, and the engine cases `jit-engine-check.mjs` replays \
         in V8 and JavaScriptCore",
    ),
    (
        "guard_traps.rs",
        "an out-of-range guest access trapping instead of reading or writing \
         the host heap",
    ),
];

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Every engine suite named above exists, and nothing else in `tests/` is
/// feature-gated without being named.
///
/// The drift guard: a file that begins `#![cfg(feature = "host-wasmtime")]` is
/// an engine suite by definition, so the two lists must agree.
#[test]
fn the_engine_suite_list_is_the_engine_suites() {
    let dir = tests_dir();
    let mut gated: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the crate's own tests/ directory") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // This file itself quotes the attribute in order to look for it, and
        // is not an engine suite. Skipping it by name is the cheap fix; the
        // guard still covers every other file in the directory.
        if path.file_name().and_then(|f| f.to_str()) == Some("engine_suites_present.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable test file");
        if text.starts_with("//!") && text.contains("\n#![cfg(feature = \"host-wasmtime\")]") {
            gated.push(
                path.file_name()
                    .expect("a named file")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    gated.sort();

    let mut named: Vec<String> = ENGINE_SUITES.iter().map(|(f, _)| (*f).to_owned()).collect();
    named.sort();

    assert_eq!(
        gated,
        named,
        "the engine-suite list in this file and the `host-wasmtime`-gated files \
         in {} have drifted. Add the new suite (with a line saying what it \
         checks) or fix the name here — the point of the list is that a reader \
         of a default `cargo test` output learns what did not run.",
        dir.display(),
    );
}

/// Say, loudly and by name, whether the engine suites ran.
///
/// Run it with `--nocapture` to see the text; `just test-emu-jit-identity` is
/// what turns the feature on.
#[test]
fn the_engine_suites_ran_or_this_says_they_did_not() {
    if cfg!(feature = "host-wasmtime") {
        println!(
            "lp-emu-jit: host-wasmtime is ON — {} engine suite(s) compiled in and run beside this one.",
            ENGINE_SUITES.len(),
        );
        return;
    }

    println!("================================================================");
    println!("SKIPPED: lp-emu-jit's engine suites are NOT compiled into this run.");
    println!();
    println!("  `host-wasmtime` is off (the default, JD18: wasmtime is 130-220 s");
    println!("  of cranelift over these images and no other job should pay for");
    println!("  it). These suites therefore contributed ZERO tests:");
    for (file, what) in ENGINE_SUITES {
        println!("    tests/{file}");
        println!("      {what}");
    }
    println!();
    println!("  Run them with:  just test-emu-jit-identity");
    println!("  (or: cargo test -p lp-emu-jit --features host-wasmtime)");
    println!("================================================================");
}

//! The one test in this crate that is **never** compiled out, and whose only
//! job is to say what is.
//!
//! The round-trip suites — the ones that run emitted modules under wasmtime
//! against a real `XtHart` and write the engine cases
//! `scripts/emu/jit-engine-check.mjs` replays in V8 and JavaScriptCore — are
//! `#![cfg(feature = "host-wasmtime")]` at file scope. Without the feature
//! they compile to nothing, and a green `cargo test -p lp-xt-jit` would read
//! as an engine agreeing. M7 JD14 says neither oracle tier may skip
//! silently; this is the default path's half of that, mirroring
//! `lp-emu-jit/tests/engine_suites_present.rs`.
//!
//! It also guards against its own drift: the list below is checked against
//! the directory, so a new engine suite that forgets to be named here fails
//! this test rather than quietly widening the hole.

use std::path::{Path, PathBuf};

/// The suites that only exist when `host-wasmtime` is on, and one line each
/// saying what stops being checked without it.
const ENGINE_SUITES: &[(&str, &str)] = &[
    (
        "seam_escape_all.rs",
        "the escape-everything module against a scripted interpreter: the exit \
         protocol, the counters, the slice end, the budget",
    ),
    (
        "translate_roundtrip.rs",
        "the integer core, the memory arms and the branches, emitted and compared \
         with the same program escaped to a real XtHart, at 1, 8 and 64 blocks a \
         function; the engine cases for V8 and JavaScriptCore",
    ),
    (
        "window_roundtrip.rs",
        "entry/retw in every increment, the ring wrapping past AR[63], the hoisted \
         overflow refusal and the underflow refusal",
    ),
    (
        "loop_roundtrip.rs",
        "loop/loopnez/loopgtz, the loop-back, a branch and a store at the loop end, \
         a moved LEND and a moved LBEG",
    ),
];

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn the_engine_suite_list_is_the_engine_suites() {
    let dir = tests_dir();
    let mut gated: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the crate's own tests/ directory") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|f| f.to_str()) == Some("engines.rs") {
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
        "the engine-suite list in this file and the `host-wasmtime`-gated files in {} have \
         drifted",
        dir.display(),
    );
}

#[test]
fn the_engine_suites_ran_or_this_says_they_did_not() {
    if cfg!(feature = "host-wasmtime") {
        println!(
            "lp-xt-jit: host-wasmtime is ON — {} engine suite(s) compiled in and run beside this one.",
            ENGINE_SUITES.len(),
        );
        return;
    }
    println!("================================================================");
    println!("SKIPPED: lp-xt-jit's engine suites are NOT compiled into this run.");
    println!();
    println!("  `host-wasmtime` is off (the default). These suites contributed ZERO tests:");
    for (file, what) in ENGINE_SUITES {
        println!("    tests/{file}");
        println!("      {what}");
    }
    println!();
    println!("  Run them with:  just test-emu-xt-jit-identity");
    println!("  (or: cargo test -p lp-xt-jit --features host-wasmtime)");
    println!("================================================================");
}

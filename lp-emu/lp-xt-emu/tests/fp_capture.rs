//! The command that diffs a device capture against the committed predictions.
//!
//! ```bash
//! just fp-diff target/fp-capture/fpconf-20260731-190000.txt
//! ```
//!
//! Shaped as a test rather than a binary so it stays inside the crate's own
//! validation — the parsing and classification live in [`lp_xt_emu::fp_capture`]
//! and are covered by that module's unit tests on every `cargo test`, board or
//! no board. This file is only the shell: it reads two files and prints.
//!
//! Without `FP_CAPTURE` it does nothing but assert that the corpus files on disk
//! parse, which is worth having on its own — a corpus file that the diff tool
//! cannot read would be discovered at the desk otherwise.
//!
//! **Never regenerate a prediction from what this prints.** A `DIVERGE` row is a
//! finding to triage; a `RESOLVED` row is an answer to be turned into a policy
//! field with a citation. Editing a golden to match the device inverts the test
//! into a tautology that passes forever (M6 D2).

use std::path::PathBuf;

use lp_xt_emu::fp_capture::{Capture, Predictions, diff, parse_capture, parse_predictions};
use lp_xt_fp_vectors::Family;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fp")
}

fn predictions(family: Family) -> Predictions {
    let path = fixtures_dir().join(format!("{}.txt", family.name()));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse_predictions(family.name(), &text)
        .unwrap_or_else(|e| panic!("{path:?} does not parse: {e}"))
}

/// The committed corpus must be readable by the same parser the campaign uses.
/// Runs everywhere, needs nothing.
#[test]
fn every_committed_corpus_file_parses_and_agrees_on_the_fingerprint() {
    let want = lp_xt_fp_vectors::fingerprint();
    for family in Family::ALL {
        let p = predictions(family);
        assert_eq!(
            p.fingerprint,
            want,
            "{}: corpus header fingerprint disagrees with the generator",
            family.name()
        );
        assert_eq!(
            p.rows.len(),
            lp_xt_fp_vectors::count(family) as usize,
            "{}: row count",
            family.name()
        );
    }
}

/// The diff itself. Silent unless `FP_CAPTURE` names a capture file.
#[test]
fn diff_a_capture_against_the_committed_predictions() {
    let Some(path) = std::env::var_os("FP_CAPTURE") else {
        println!("fp_capture: set FP_CAPTURE=<path> to diff a device capture");
        return;
    };
    let path = PathBuf::from(path);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));

    // Both aborts happen here, before a single row is classified.
    let capture: Capture = match parse_capture(&text) {
        Ok(c) => c,
        Err(e) => panic!("{}: {e}", path.display()),
    };

    println!("=== FP conformance diff: {} ===", path.display());
    println!(
        "device fingerprint {:#010x}   host {:#010x}",
        capture.fingerprint,
        lp_xt_fp_vectors::fingerprint()
    );
    println!(
        "firmware commit    {}",
        capture.commit.as_deref().unwrap_or("<unknown>")
    );
    println!(
        "cpenable           before={:#010x} after={:#010x}",
        capture.cpenable_before.unwrap_or(0),
        capture.cpenable_after.unwrap_or(0)
    );
    println!(
        "reset              FCR={:#010x} FSR={:#010x}",
        capture.fcr_reset.unwrap_or(0),
        capture.fsr_reset.unwrap_or(0)
    );
    println!(
        "families captured  {}",
        capture.family_names().collect::<Vec<_>>().join(", ")
    );
    println!();

    let (mut compared, mut agree, mut diverge, mut resolved, mut skipped) = (0, 0, 0, 0, 0);
    for family in Family::ALL {
        if !capture.rows.contains_key(family.name()) {
            continue;
        }
        let report = match diff(&predictions(family), &capture) {
            Ok(r) => r,
            Err(e) => panic!("{}: {e}", family.name()),
        };
        print!("{report}");
        compared += report.compared;
        agree += report.agree;
        diverge += report.diverge;
        resolved += report.resolved;
        skipped += report.skipped;
    }

    println!();
    println!(
        "TOTAL compared={compared} AGREE={agree} DIVERGE={diverge} \
         RESOLVED={resolved} SKIPPED={skipped}"
    );
    assert!(compared > 0, "the capture contained no comparable rows");

    // Deliberately NOT an assertion on `diverge == 0`. A divergence is the
    // campaign's product, not its failure: P6 triages each one into an emulator
    // bug, a harness bug, or a documented silicon behavior. Failing here would
    // push the next person toward editing a golden to get green.
    if diverge > 0 {
        println!(
            "\n{diverge} divergence(s) above. Each is a finding to triage — an \
             emulator bug, a harness bug, or real silicon behavior. Do not edit \
             a golden to make them go away."
        );
    }
}

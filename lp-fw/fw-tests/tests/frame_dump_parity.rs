//! The three firmwares' `frame_dump` modules emit the same lines.
//!
//! `[OUT] dump … rgb=…` is a wire format in everything but name. Three
//! parsers read it with no per-chip branch —
//! `scripts/m4-hardware-walk.sh` (the hardware walk, all three chips),
//! `scripts/emu/m4-walk.sh` (its emulator twin on the C6), and
//! `lp-app/lpa-server/tests/shader_oracle_frame.rs` (the host oracle, which
//! mirrors the shapes so the two transcripts line up without either being
//! sliced by hand) — and the walk's verdict is a byte comparison of what they
//! extract.
//!
//! The module itself is duplicated three times on purpose: `fw-esp32s3`,
//! `fw-esp32v3` and `fw-esp32c6` are separate crates under separate
//! toolchains with no common chip-side library, so there is nowhere to put a
//! shared copy that all three can reach. What duplication costs is the
//! failure mode this test exists to remove: a format string, a constant or a
//! dump rule changed in one copy and not the others, discovered as a walk that
//! reports "the device and the oracle differ" on a chip whose renderer is
//! perfectly correct.
//!
//! So: the **code** of the three copies must be identical, character for
//! character. Only the `//!` module documentation may differ, because each
//! chip's header explains why that chip has one — the S3's M4 gate, the
//! classic's D-bus code-install premise, the C6's emulator twin. Everything
//! below the header (the `use`, the constants, `FrameDump`, the two
//! `Display` adapters, the checksum) is compared verbatim.
//!
//! A deliberate change is therefore three edits and a green test, which is
//! the point: the moment you have to touch all three, you notice that you are
//! changing something four parsers depend on.

use std::path::{Path, PathBuf};

/// Repository root, from this crate's manifest directory (`lp-fw/fw-tests`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lp-fw/fw-tests is two levels below the repository root")
        .to_path_buf()
}

/// The three copies, by the chip whose walk reads them.
const COPIES: &[(&str, &str)] = &[
    (
        "fw-esp32s3",
        "lp-fw/fw-esp32s3/src/output/rmt/frame_dump.rs",
    ),
    (
        "fw-esp32v3",
        "lp-fw/fw-esp32v3/src/output/rmt/frame_dump.rs",
    ),
    (
        "fw-esp32c6",
        "lp-fw/fw-esp32c6/src/output/rmt/frame_dump.rs",
    ),
];

/// Everything but the `//!` module header: the code the parsers depend on.
///
/// Inner doc comments are stripped rather than the file being split at the
/// first non-`//!` line, so a header that grows a blank `//!` line in the
/// middle — or a copy that puts its header in a different order — still
/// compares equal on the half that matters.
fn code_of(path: &Path) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} is a committed source file: {e}", path.display()));
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[test]
fn the_three_firmware_frame_dump_modules_are_the_same_code() {
    let root = repo_root();
    let (first_chip, first_path) = COPIES[0];
    let first = code_of(&root.join(first_path));
    assert!(
        first.contains("[OUT] dump frame="),
        "{first_chip}: the dump line shape is gone — every walk parser greps for it"
    );
    assert!(
        first.contains("rgb={}"),
        "{first_chip}: the `rgb=` token is gone — it is what the walk compares"
    );

    for (chip, path) in &COPIES[1..] {
        let other = code_of(&root.join(path));
        if other == first {
            continue;
        }
        // A diff by line, because a 160-line assert_eq! of two near-identical
        // files is unreadable and the first differing line is the finding.
        let mismatch = first
            .lines()
            .zip(other.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| {
                format!(
                    "line {} of the code:\n  {first_chip}: {a}\n  {chip}: {b}",
                    i + 1
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "same {} lines then a length difference ({} vs {})",
                    first.lines().count().min(other.lines().count()),
                    first.lines().count(),
                    other.lines().count(),
                )
            });
        panic!(
            "{chip}'s frame_dump code differs from {first_chip}'s.\n\n{mismatch}\n\n\
             The three copies must stay identical below the `//!` header: \
             scripts/m4-hardware-walk.sh, scripts/emu/m4-walk.sh and \
             lp-app/lpa-server/tests/shader_oracle_frame.rs all parse these \
             lines with no per-chip branch, so a change here is three edits \
             (and probably a fourth on the host side), never one."
        );
    }
}

/// Every copy points at the other two.
///
/// The headers are the only place a reader of one file learns that the other
/// two exist, and a copy that does not name its siblings is how the fourth
/// chip's port will be written without anyone noticing there is a rule. The
/// first test catches a drifted format string; this one catches the drift in
/// the documentation that would have prevented it.
#[test]
fn every_copy_points_at_the_other_two() {
    let root = repo_root();
    for (chip, path) in COPIES {
        let text = std::fs::read_to_string(root.join(path)).expect("a committed source file");
        let header: String = text
            .lines()
            .take_while(|line| line.trim_start().starts_with("//!") || line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        for (sibling, sibling_path) in COPIES {
            if sibling == chip {
                continue;
            }
            assert!(
                header.contains(sibling_path),
                "{chip}'s frame_dump header does not name {sibling}'s copy \
                 ({sibling_path}). Each of the three must point at the other \
                 two: the headers are where the next porter learns the code \
                 below them is duplicated on purpose and must stay identical."
            );
        }
    }
}

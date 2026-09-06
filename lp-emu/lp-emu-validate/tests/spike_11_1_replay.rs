//! Spike report §11.1, reproduced by code.
//!
//! Two committed transcripts of the same payload built from the same ELF —
//! `test_shader_compile_incremental,esp32c6,spike_uart0_link` at commit
//! `d6cfaa2051ae` — one from the desk XIAO C6 and one from esp-emu 0.42.0. The
//! report's table said: every memory field identical, every timing field
//! wrong, and the harness's own 5 ms slice budget passes under the emulator
//! and fails on the board.
//!
//! That table was a human reading two files. This is the same claim, checked
//! on every `cargo test` with no board attached, forever — the discipline
//! `lp-emu/lp-xt-emu/tests/fp_silicon_replay.rs` established for the FP
//! campaign.
//!
//! **Never edit a transcript.** If one of these assertions starts failing, the
//! answer is a regression in the parser or a re-capture with its own header —
//! never a digit changed in a `.txt`.

use std::path::{Path, PathBuf};

use lp_emu_validate::grade::{FieldClass, Grade};
use lp_emu_validate::replay::{ReplayOptions, replay};
use lp_emu_validate::transcript::Transcript;
use lp_emu_validate::{TranscriptHeader, payload};

const SILICON: &str = "silicon-seeed-xiao-esp32-c6-2026-09-06-d6cfaa205.txt";
const ESP_EMU: &str = "esp-emu-0.42.0-2026-09-06-d6cfaa205.txt";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32c6/shader-compile-stress")
}

fn load(name: &str) -> Transcript {
    let path = dir().join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

/// Reload a transcript with its body altered — the negative-control vehicle.
/// The file on disk is never touched.
fn load_with_body(name: &str, edit: impl Fn(String) -> String) -> Transcript {
    let path = dir().join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    let meta = std::fs::read_to_string(lp_emu_validate::transcript::sidecar_path(&path)).unwrap();
    let header = TranscriptHeader::from_json(&meta).unwrap();
    Transcript::from_parts(header, &edit(body)).unwrap()
}

fn ratio(report: &lp_emu_validate::ReplayReport, scope: &str, field: &str) -> f64 {
    report
        .comparisons
        .iter()
        .find(|c| c.scope == scope && c.field == field)
        .unwrap_or_else(|| panic!("no comparison {scope}.{field}"))
        .ratio
        .unwrap_or_else(|| panic!("{scope}.{field} has no ratio"))
}

/// Both transcripts load, and their headers say what §11 says they say.
#[test]
fn the_pair_carries_its_provenance() {
    let s = load(SILICON);
    let e = load(ESP_EMU);

    assert_eq!(s.header.configuration, "silicon:seeed/xiao-esp32-c6");
    assert_eq!(e.header.configuration, "esp-emu:0.42.0");
    assert_eq!(s.header.firmware_commit, e.header.firmware_commit);
    assert_eq!(s.header.firmware_features, e.header.firmware_features);
    assert!(
        s.header
            .firmware_features
            .contains(&"spike_uart0_link".to_string())
    );

    // The disagreement the spike found, carried rather than corrected.
    assert_eq!(s.header.silicon_rev.as_deref(), Some("v0.2"));
    assert_eq!(e.header.silicon_rev.as_deref(), Some("v0.3"));
    assert_eq!(s.header.mac.as_deref(), Some("a0:f2:62:87:b4:8c"));

    // Both reached `=== DONE ===`.
    assert!(s.sentinel_line().is_some(), "silicon reached the sentinel");
    assert!(e.sentinel_line().is_some(), "esp-emu reached the sentinel");
}

/// The memory half of the §11.1 table: every field identical.
#[test]
fn memory_is_byte_equal_across_the_pair() {
    let s = load(SILICON);
    let e = load(ESP_EMU);
    let report = replay(&s, &e, ReplayOptions::default()).unwrap();

    let differing: Vec<_> = report.differences_in(FieldClass::Memory).collect();
    assert!(
        differing.is_empty(),
        "expected zero memory differences, got {}:\n{:#?}",
        differing.len(),
        differing
    );
    assert!(
        report.structural_problems.is_empty(),
        "{:#?}",
        report.structural_problems
    );

    // 92 ticks x 4 heap numbers + peak/resident/after_drop + worst_peak_used.
    assert_eq!(report.compared(FieldClass::Memory), 92 * 4 + 3 + 1);

    // The report's own phrasing: "all 184 values" is the per-tick `used`
    // halves, mem_before and mem_after, over 92 ticks.
    let used: Vec<_> = report
        .comparisons_in(FieldClass::Memory)
        .filter(|c| c.scope.starts_with("compile-tick[") && c.field.ends_with("_used"))
        .collect();
    assert_eq!(used.len(), 184);
    assert!(used.iter().all(|c| c.equal));

    // And the three figures the report quoted by name.
    for (field, want) in [
        ("peak_used", "48132"),
        ("resident_used", "18932"),
        ("after_drop_used", "3976"),
    ] {
        let c = report
            .comparisons
            .iter()
            .find(|c| c.scope == "case-summary" && c.field == field)
            .unwrap();
        assert_eq!(c.left, want, "silicon {field}");
        assert_eq!(c.right, want, "esp-emu {field}");
    }

    assert!(report.is_ok(), "{}", report.render());
}

/// The timing half: every field divergent, with the ratios the report quoted.
#[test]
fn timing_diverges_with_the_ratios_the_spike_measured() {
    let s = load(SILICON);
    let e = load(ESP_EMU);
    let report = replay(&s, &e, ReplayOptions::default()).unwrap();

    // Divergent, not silently equal.
    let differing = report.differences_in(FieldClass::Timing).count();
    let compared = report.compared(FieldClass::Timing);
    assert_eq!(
        compared,
        92 * 2 + 2 + 2,
        "slice_cycles/slice_us + summaries"
    );
    assert_eq!(
        differing, compared,
        "every timing field should differ; the emulator has no cycle model"
    );

    // §11.1: build_us 568,757 vs 54,361 = 10.5x (contaminated by the tee).
    assert!(
        (ratio(&report, "case-summary", "build_us") - 10.46).abs() < 0.01,
        "build_us ratio {}",
        ratio(&report, "case-summary", "build_us")
    );
    // §11.1: max_slice_us 11,724 vs 4,625 = 2.5x.
    assert!(
        (ratio(&report, "case-summary", "max_slice_us") - 2.53).abs() < 0.01,
        "max_slice_us ratio {}",
        ratio(&report, "case-summary", "max_slice_us")
    );

    // The clean number, and the contaminated one, computed from the series the
    // replay parsed. Ticks 1-18 carry no log line inside the slice; ticks 19-92
    // each log an 81-byte line the ROM UART pushes at 115,200 baud (7.03 ms).
    let (sil_1_18, emu_1_18) = window_sum(&report, 1..=18);
    let (sil_19_92, emu_19_92) = window_sum(&report, 19..=92);
    assert_eq!((sil_1_18, emu_1_18), (42_663.0, 18_003.0));
    assert_eq!((sil_19_92, emu_19_92), (526_048.0, 36_312.0));
    assert!(
        (sil_1_18 / emu_1_18 - 2.37).abs() < 0.005,
        "ticks 1-18 ratio {}",
        sil_1_18 / emu_1_18
    );
    assert!(
        (sil_19_92 / emu_19_92 - 14.49).abs() < 0.01,
        "ticks 19-92 ratio {}",
        sil_19_92 / emu_19_92
    );

    // The consequence, stated as an assertion rather than a paragraph: the
    // harness's own 5 ms slice budget passes under the emulator and fails on
    // the board. A host gate built on emulated slice_us would green-light a
    // build the desk rejects (plan PD9).
    const SLICE_BUDGET_US: f64 = 5000.0;
    let max_slice = report
        .comparisons
        .iter()
        .find(|c| c.scope == "case-summary" && c.field == "max_slice_us")
        .unwrap();
    assert!(max_slice.left.parse::<f64>().unwrap() > SLICE_BUDGET_US);
    assert!(max_slice.right.parse::<f64>().unwrap() < SLICE_BUDGET_US);

    // Timing divergence does not fail a replay.
    assert!(report.is_ok(), "{}", report.render());

    // But it is visible: the rendered report carries the ratios.
    let text = report.render();
    assert!(text.contains("timing divergence"), "{text}");
    assert!(text.contains("10.46x"), "{text}");
    assert!(text.contains("REPLAY OK"), "{text}");
}

/// Sum `slice_us` over an inclusive tick window, on both sides.
fn window_sum(
    report: &lp_emu_validate::ReplayReport,
    ticks: std::ops::RangeInclusive<u32>,
) -> (f64, f64) {
    let mut left = 0.0;
    let mut right = 0.0;
    for tick in ticks {
        let scope = format!("compile-tick[{tick}]");
        let c = report
            .comparisons
            .iter()
            .find(|c| c.scope == scope && c.field == "slice_us")
            .unwrap_or_else(|| panic!("no {scope}.slice_us"));
        left += c.left.parse::<f64>().unwrap();
        right += c.right.parse::<f64>().unwrap();
    }
    (left, right)
}

/// Strict mode refuses the unmeasured claim, and names it.
#[test]
fn strict_mode_refuses_the_modeled_timing_claim() {
    let s = load(SILICON);
    let e = load(ESP_EMU);

    assert_eq!(s.header.trust.grade(FieldClass::Timing), Grade::Measured);
    assert_eq!(e.header.trust.grade(FieldClass::Timing), Grade::Modeled);

    let report = replay(
        &s,
        &e,
        ReplayOptions {
            strict: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(!report.is_ok(), "strict mode should refuse this pair");
    let problems = report.grade_problems.join("\n");
    assert!(problems.contains("timing"), "{problems}");
    assert!(problems.contains("esp-emu:0.42.0"), "{problems}");
    assert!(problems.contains("no cycle model"), "{problems}");
    // Memory is measured on both sides, so strict mode has nothing to say
    // about it — that is the point of grading per class.
    assert!(
        !problems.lines().any(|l| l.contains("for memory")),
        "{problems}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls
// ---------------------------------------------------------------------------

/// One digit of `peak_used`, and the replay must fail.
#[test]
fn a_corrupted_peak_used_digit_fails_the_replay() {
    let s = load(SILICON);
    let e = load_with_body(ESP_EMU, |body| {
        let corrupted = body.replacen(r#""peak_used":48132"#, r#""peak_used":48133"#, 1);
        assert_ne!(corrupted, body, "the corruption must actually apply");
        corrupted
    });

    let report = replay(&s, &e, ReplayOptions::default()).unwrap();
    assert!(!report.is_ok(), "a corrupted peak_used must fail");

    let failures = report.failures().join("\n");
    assert!(failures.contains("peak_used"), "{failures}");
    assert!(failures.contains("48132"), "{failures}");
    assert!(failures.contains("48133"), "{failures}");
    assert!(report.render().contains("REPLAY FAILED"));
}

/// The same for a per-tick heap value, which no summary would catch.
#[test]
fn a_corrupted_per_tick_heap_value_fails_the_replay() {
    let s = load(SILICON);
    let e = load_with_body(ESP_EMU, |body| {
        let corrupted = body.replacen(
            "tick=47 stage= slice_cycles",
            "tick=47 stage= slice_cycles",
            1,
        );
        // Move one byte of tick 47's mem_after, leaving every summary intact.
        let line = corrupted
            .lines()
            .find(|l| l.contains("tick=47 "))
            .expect("tick 47 is in the transcript")
            .to_string();
        let broken = line.replace(" used mem_after=", " used mem_after=1");
        assert_ne!(broken, line);
        corrupted.replace(&line, &broken)
    });

    let report = replay(&s, &e, ReplayOptions::default()).unwrap();
    assert!(!report.is_ok(), "a corrupted per-tick heap value must fail");
    let failures = report.failures().join("\n");
    assert!(failures.contains("compile-tick[47]"), "{failures}");
    assert!(failures.contains("mem_after_free"), "{failures}");
}

/// A missing sentinel is a structural failure, not a silently short run.
#[test]
fn a_truncated_transcript_fails_the_replay() {
    let s = load(SILICON);
    let e = load_with_body(ESP_EMU, |body| {
        body.replace("[inc-shader-compile] === DONE ===", "")
    });
    let report = replay(&s, &e, ReplayOptions::default()).unwrap();
    assert!(!report.is_ok());
    assert!(
        report
            .structural_problems
            .iter()
            .any(|p| p.contains("sentinel")),
        "{:#?}",
        report.structural_problems
    );
}

/// The masked view is what `mask-transcript.sh` produced, and the file behind
/// it is untouched.
#[test]
fn masking_makes_the_boot_lines_comparable_without_editing_anything() {
    let e = load(ESP_EMU);
    let set = lp_emu_validate::mask_set(
        payload::find_payload("shader-compile-stress")
            .unwrap()
            .mask_set,
    )
    .unwrap();
    let masked = e.masked(set);

    // esp-emu colours its bootloader lines; masking removes the escapes and the
    // millisecond stamp, and leaves the content.
    let boot = masked
        .iter()
        .find(|l| l.contains("chip revision"))
        .expect("the bootloader banner is in the transcript");
    assert_eq!(boot, "I (N) boot: chip revision: v0.3");

    // The bytes on disk still carry the escape.
    let raw = std::fs::read_to_string(dir().join(ESP_EMU)).unwrap();
    assert!(
        raw.contains('\u{1b}'),
        "the committed file keeps its escapes"
    );
    assert!(raw.contains('\r'), "and its carriage returns");
}

/// Every committed transcript loads, and its filename matches its header.
///
/// This is the contract enforcement: a transcript filed under the wrong
/// configuration or date is a transcript nobody can trust.
#[test]
fn every_committed_transcript_is_filed_where_its_header_says() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts");
    let mut seen = 0;
    visit(&root, &mut |path| {
        if path.extension().is_none_or(|e| e != "txt") {
            return;
        }
        let t =
            Transcript::load(path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()));
        let want = root.join(t.header.relative_path().unwrap());
        assert_eq!(
            path.canonicalize().unwrap(),
            want.canonicalize().unwrap_or(want.clone()),
            "{} should be filed at {}",
            path.display(),
            want.display()
        );
        seen += 1;
    });
    assert!(seen >= 2, "expected the §11.1 pair, found {seen}");
}

fn visit(dir: &Path, f: &mut impl FnMut(&Path)) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, f);
        } else {
            f(&path);
        }
    }
}

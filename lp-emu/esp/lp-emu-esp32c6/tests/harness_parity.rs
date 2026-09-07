//! G6-1, the milestone's strongest gate: the shader-compile harness runs on
//! the machine and its transcript replays **byte-equal in the memory class**
//! against the committed silicon capture — `heap_start 321600 free/3936
//! used`, `peak_used 48132`, `resident_used 18932`, `after_drop_used 3976`
//! and all 92 `mem_before`/`mem_after` pairs. Timing is reported with its
//! ratio, never compared (plan PD9).
//!
//! `#[ignore]`d: needs the harness reference image
//! (`scripts/emu/build-reference-image.sh
//! test_shader_compile_incremental,esp32c6,spike_uart0_link`; `just
//! test-emu-c6` builds it, or set `LP_EMU_C6_REF_HARNESS`). See
//! `test_support::reference_image`.

use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade, Uart0Sink,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice, workspace_root};
use lp_emu_validate::{FieldClass, ReplayOptions, Transcript, TranscriptHeader, replay};

const DONE: &str = "[inc-shader-compile] === DONE ===";
const SILICON: &str =
    "lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-06-d6cfaa205.txt";
/// What M3 P7's runner recorded on this machine, committed beside it.
const COMMITTED_T1: &str =
    "lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t1-2026-09-06-d6cfaa205.txt";

/// The sidecar a capture from this machine carries. The configuration is
/// the one `validate.toml` names for M3; the trust table says memory is
/// measured (same image bytes, byte-exact RAM) and time is modeled (t1).
const SIDECAR: &str = r#"{
  "schema": 1,
  "payload": "shader-compile-stress",
  "chip": "esp32c6",
  "configuration": "lp-emu:esp32c6:t1",
  "date": "2026-09-06",
  "firmware_commit": "d6cfaa2051ae",
  "firmware_features": ["test_shader_compile_incremental", "esp32c6", "spike_uart0_link"],
  "firmware_dirty": false,
  "silicon_rev": "v0.2",
  "board": "seeed/xiao-esp32-c6",
  "mac": "a0:f2:62:87:b4:8c",
  "tools": { "lp-emu-esp32c6": "M3 P6 harness_parity test" },
  "source": "tests/harness_parity.rs on target/emu-ref/d6cfaa205-harness/fw-esp32c6",
  "capture": "in-process: strict bus, t1, default eFuse identity, no host input, --exit-on the DONE line",
  "trust": [
    { "class": "memory", "grade": "measured", "because": "the same image bytes on a byte-exact RAM model" },
    { "class": "timing", "grade": "modeled", "because": "t1 counts instructions; the UART drain is modelled at baud" }
  ]
}"#;

struct Run {
    m: Esp32C6Machine,
    outcome: Outcome,
    text: String,
}

fn run_harness() -> Option<Run> {
    run_harness_at(TimeGrade::T1)
}

fn run_harness_at(grade: TimeGrade) -> Option<Run> {
    let elf = match reference_image(&ReferenceImage::HARNESS) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("harness_parity", &reason);
            return None;
        }
    };
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(grade)
        .uart0(Uart0Sink::Memory)
        .build()
        .expect("the harness image builds a machine");
    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(20_000_000 * memmap::CYCLES_PER_US),
        exit_on: Some(DONE.to_string()),
        wall_timeout: None,
        probes: Vec::new(),
    });
    let text = String::from_utf8_lossy(&m.uart0().bytes()).into_owned();
    Some(Run { m, outcome, text })
}

#[test]
#[ignore = "needs the harness reference image; run through `just test-emu-c6`"]
fn the_harness_transcript_is_byte_equal_to_silicon_in_the_memory_class() {
    let Some(Run { m, outcome, text }) = run_harness() else {
        return;
    };
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "expected the DONE line, got {outcome:?}"
    );
    assert_eq!(m.bus.unmapped_reads() + m.bus.unmapped_writes(), 0);
    assert!(text.contains(DONE));

    let root = workspace_root().expect("workspace root");
    let ours = Transcript::from_parts(
        TranscriptHeader::from_json(SIDECAR).expect("the sidecar parses"),
        &text,
    )
    .expect("our capture is a transcript");
    let silicon = Transcript::load(root.join(SILICON)).expect("the committed silicon transcript");
    let report = replay(
        &ours,
        &silicon,
        ReplayOptions {
            strict: false,
            strict_timing: false,
        },
    )
    .expect("the replay runs");
    // The timing ratio table the phase report quotes; `--nocapture` shows it.
    println!("{}", report.render());

    assert!(report.is_ok(), "{:?}", report.failures());
    // 372 memory values: 184 per-tick pairs plus the summaries.
    assert_eq!(
        report.compared(FieldClass::Memory),
        372,
        "memory values compared"
    );
    assert_eq!(
        report.differences_in(FieldClass::Memory).count(),
        0,
        "memory-class differences: {:?}",
        report
            .differences_in(FieldClass::Memory)
            .map(|d| format!("{d:?}"))
            .collect::<Vec<_>>()
    );
    // Timing is reported, not compared: present and not a failure.
    assert!(report.compared(FieldClass::Timing) > 0);
    // The numbers the brief names, straight from the text as well.
    for needle in [
        "heap_start=321600 free/3936 used",
        "heap_peak_used=48132",
        "heap_resident=306604 free/18932 used",
        "after_drop=321560 free/3976 used",
    ] {
        assert!(text.contains(needle), "missing `{needle}`");
    }
    assert_eq!(text.matches("mem_before").count(), 92, "92 ticks");
}

/// A time grade must not move a heap byte.
///
/// `t1` counts one cycle per instruction and `t2` uses the measured per-class
/// model, so every timing figure in the harness moves between them. Every
/// memory figure must not: the allocator sees the same calls in the same order
/// whatever the clock says, and this payload is single-task, so there is no
/// interleaving for a grade to change. (The idle boot is a different case —
/// its stack high-water mark *is* grade-dependent, 11,432 B under `t1` and
/// 11,752 B under `t2`, because there the deepest interrupted call chain
/// depends on when the interrupts landed. That is why this test is scoped to
/// the harness and `tests/boot_idle.rs` states the other half.)
///
/// The `t2` capture is compared against the **committed** `t1` transcript, so
/// what this checks is a fact about the recorded artefact, not about two runs
/// in one process.
#[test]
#[ignore = "needs the harness reference image; run through `just test-emu-c6`"]
fn t2_memory_equals_t1() {
    let Some(Run { m, outcome, text }) = run_harness_at(TimeGrade::T2) else {
        return;
    };
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{outcome:?}"
    );
    assert_eq!(m.bus.unmapped_reads() + m.bus.unmapped_writes(), 0);

    let root = workspace_root().expect("workspace root");
    let sidecar = SIDECAR.replace("lp-emu:esp32c6:t1", "lp-emu:esp32c6:t2");
    let ours = Transcript::from_parts(
        TranscriptHeader::from_json(&sidecar).expect("the sidecar parses"),
        &text,
    )
    .expect("our t2 capture is a transcript");
    let recorded = Transcript::load(root.join(COMMITTED_T1)).expect("the committed t1 transcript");
    let report = replay(
        &ours,
        &recorded,
        ReplayOptions {
            strict: false,
            strict_timing: false,
        },
    )
    .expect("the replay runs");
    println!("{}", report.render());

    assert!(report.is_ok(), "{:?}", report.failures());
    assert_eq!(report.compared(FieldClass::Memory), 372);
    assert_eq!(
        report.differences_in(FieldClass::Memory).count(),
        0,
        "a time grade moved a heap byte: {:?}",
        report
            .differences_in(FieldClass::Memory)
            .map(|d| format!("{}.{}: {} vs {}", d.scope, d.field, d.left, d.right))
            .collect::<Vec<_>>()
    );
    // Time did move, or the grades would be the same thing.
    assert!(
        report.differences_in(FieldClass::Timing).count() > 0,
        "t2 produced t1's timings"
    );
}

#[test]
#[ignore = "needs the harness reference image; run through `just test-emu-c6`"]
fn two_harness_runs_are_byte_identical() {
    // G6-4, the harness half: same bytes, same final cycle.
    let (Some(a), Some(b)) = (run_harness(), run_harness()) else {
        return;
    };
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.text, b.text, "two runs of the harness diverged");
}

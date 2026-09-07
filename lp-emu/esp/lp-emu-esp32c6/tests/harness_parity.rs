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
        .time_grade(TimeGrade::T1)
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

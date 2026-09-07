//! M3's gates, as replays of committed transcripts.
//!
//! The machine's own tests (`lp-emu/esp/lp-emu-esp32c6/tests/`) run the
//! firmware and are `#[ignore]`d for it. These need no firmware and no board:
//! they read what the runner recorded, beside what the desk and esp-emu
//! recorded, and check the claims the milestone is allowed to make. That is
//! the point of committing a transcript — the gate outlives the sitting.
//!
//! ```bash
//! cargo run -p lp-cli -- validate record emu-m3 --config lp-emu:esp32c6:t1 \
//!   --date 2026-09-06 --commit d6cfaa2051ae --dirty --timeout-secs 20 \
//!   --image shader-compile-stress=target/emu-ref/d6cfaa205-harness/fw-esp32c6 \
//!   --image boot-idle=target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6
//! ```
//!
//! **Never edit a transcript.** A failure here is a regression or a
//! re-capture with its own header, never a digit changed in a `.txt`.

use std::path::PathBuf;

use lp_emu_validate::grade::{FieldClass, Grade};
use lp_emu_validate::replay::{ReplayOptions, ReplayReport, replay};
use lp_emu_validate::transcript::{Transcript, sidecar_path};
use lp_emu_validate::{TranscriptHeader, find_payload};

const OURS: &str = "lp-emu-esp32c6-t1-2026-09-06-d6cfaa205.txt";
const SILICON: &str = "silicon-esp32c6-2026-09-06-d6cfaa205.txt";
const ESP_EMU: &str = "esp-emu-0.42.0-2026-09-06-d6cfaa205.txt";

fn transcripts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32c6")
}

fn load(payload: &str, name: &str) -> Transcript {
    let path = transcripts().join(payload).join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

/// Reload a transcript with its body altered. The file on disk is never
/// touched — this is the negative control's vehicle, not an edit.
fn load_with_body(payload: &str, name: &str, edit: impl Fn(String) -> String) -> Transcript {
    let path = transcripts().join(payload).join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    let header =
        TranscriptHeader::from_json(&std::fs::read_to_string(sidecar_path(&path)).unwrap())
            .unwrap();
    Transcript::from_parts(header, &edit(body)).unwrap()
}

fn replay_ours_against(payload: &str, other: &str) -> ReplayReport {
    let report = replay(
        &load(payload, OURS),
        &load(payload, other),
        ReplayOptions::default(),
    )
    .expect("the replay runs");
    // The ratio table the phase report quotes; `--nocapture` prints it.
    println!("{}", report.render());
    report
}

/// The milestone's first gate: the compile harness on our machine against the
/// desk board, from the two committed files.
///
/// Memory is the claim — 372 values: 4 per tick over 92 ticks, plus
/// `peak_used`, `resident_used`, `after_drop_used` and `worst_peak_used`.
/// Time is *reported*, never compared (plan PD9), and the ratios are the
/// first point on the graded ladder.
#[test]
fn lp_emu_t1_vs_silicon_shader_compile_stress() {
    let report = replay_ours_against("shader-compile-stress", SILICON);

    assert!(report.is_ok(), "{:?}", report.failures());
    assert_eq!(report.compared(FieldClass::Memory), 372);
    assert_eq!(
        report.differences_in(FieldClass::Memory).count(),
        0,
        "{:?}",
        report
            .differences_in(FieldClass::Memory)
            .map(|d| format!("{}.{}: {} vs {}", d.scope, d.field, d.left, d.right))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.compared(FieldClass::Structural), 190);
    assert_eq!(report.differences_in(FieldClass::Structural).count(), 0);

    // Timing: present, divergent, and not a failure.
    assert!(report.compared(FieldClass::Timing) > 0);
    assert!(report.differences_in(FieldClass::Timing).count() > 0);
    let build = ratio(&report, "total-summary", "build_us");
    assert!(
        (0.95..1.00).contains(&build),
        "build_us ratio {build} — P6 measured 0.97x; the UART drain at baud is why"
    );
}

/// The same, against the second oracle. esp-emu is graded `measured` for
/// memory on this payload, which is what makes it worth replaying against:
/// three configurations, one set of heap numbers.
#[test]
fn lp_emu_t1_vs_esp_emu_shader_compile_stress() {
    let report = replay_ours_against("shader-compile-stress", ESP_EMU);

    assert!(report.is_ok(), "{:?}", report.failures());
    assert_eq!(report.differences_in(FieldClass::Memory).count(), 0);

    // The compute ticks are byte-identical in *time* as well, because both
    // configurations count one cycle per instruction there. The log-bearing
    // ticks are not: esp-emu's ROM UART drains for free and ours does not.
    let identical = report
        .comparisons_in(FieldClass::Timing)
        .filter(|c| c.from_series && c.field == "slice_us" && c.equal)
        .count();
    assert_eq!(
        identical, 18,
        "ticks 1-18 are the compute ticks; both grades count instructions there"
    );
}

/// The idle boot, against spike report §5.4 — the memfs variant of the shipped
/// image, which is the M3 gate image (DD23: the flash-backed one stops on
/// `SPI1.cmd` until M4).
///
/// Two of these three figures are §5.4's exactly. The third is not, and is
/// pinned here **as measured** with its reason: at the 5 s heartbeat this
/// machine reads `freeBytes 266688` against esp-emu's 266,792 — 104 B — and
/// M3 P6 established that it is a transient in silent guest state, not a
/// loader fact (the same run's 15 s heartbeat reads 266,788, 4 B away, which
/// is the same 4 B the spike report saw between esp-emu and silicon on one
/// image in §11.2; the figure does not move with the time grade; and a
/// throwaway "SOF forever" USB model, esp-emu's §4 behaviour, left it
/// unchanged). Only a silicon capture of this variant can arbitrate it — a G3
/// sitting-1 item (DD26). **Never tuned toward 266,792.**
#[test]
fn boot_idle_matches_spike_5_4() {
    let t = load("boot-idle", OURS);
    assert!(
        t.sentinel_line().is_some(),
        "the run reached the stack heartbeat"
    );

    let payload = find_payload("boot-idle").unwrap();
    let by_name = |name: &str| {
        *payload
            .series
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("series `{name}`"))
    };

    let hello = t.series(by_name("hello"));
    assert_eq!(hello.len(), 1, "one hello per boot");
    assert_eq!(hello[0].values["proto"], "20");
    assert_eq!(hello[0].values["board_id"], "seeed/xiao-esp32-c6");

    let beat = t.series(by_name("heartbeat"));
    assert_eq!(beat.len(), 1, "the sentinel stops the run at the first one");
    let m = &beat[0].values;
    // §5.4 exactly.
    assert_eq!(m["total_bytes"], "325536");
    // The 104 B deferral, pinned as measured on this configuration.
    assert_eq!(m["free_bytes"], "266688", "esp-emu §5.4 read 266792");
    assert_eq!(m["used_bytes"], "58848");
    // Direct load seeds LP_CLKRST's reset cause, so the ROM's
    // `rtc_get_reset_reason` answers POWERON and the firmware agrees.
    assert_eq!(m["reset_reason"], "power-on");
    assert_eq!(m["boot_count"], "1");
    assert_eq!(m["loaded_projects"], "");

    let stack = t.series(by_name("stack-heartbeat"));
    assert_eq!(stack.len(), 1);
    // §5.4 exactly, under `t1`. Under `t2` the high-water mark is 11,752 B:
    // interleaving decides the deepest interrupted call chain, which is why
    // this figure is grade-dependent and the harness is where a time grade is
    // held to moving no heap byte at all.
    assert_eq!(stack[0].key, "71960");
    assert_eq!(stack[0].values["high_water"], "11432");
    assert_eq!(stack[0].values["headroom"], "60528");

    // The `[FS]` mount pair is M4's (DD23), on the flash-backed image.
    assert!(
        !t.lines.iter().any(|l| l.contains("[FS]")),
        "the memfs variant mounts nothing"
    );
}

/// The negative control. A single wrong digit in a heap figure has to fail,
/// or none of the above means anything.
#[test]
fn a_corrupted_heap_digit_fails_the_replay() {
    // The harness's summary: `peak_used` 48132 -> 48133.
    let ours = load_with_body("shader-compile-stress", OURS, |body| {
        body.replace(r#""peak_used":48132"#, r#""peak_used":48133"#)
    });
    let report = replay(
        &ours,
        &load("shader-compile-stress", SILICON),
        ReplayOptions::default(),
    )
    .unwrap();
    let failures = report.failures();
    assert!(!report.is_ok(), "a wrong peak_used must fail");
    assert!(
        failures.iter().any(|f| f.contains("peak_used")),
        "{failures:?}"
    );

    // And a per-tick figure, which is where 184 of the 372 values live.
    let ours = load_with_body("shader-compile-stress", OURS, |body| {
        body.replace("mem_after=308508", "mem_after=308504")
    });
    let report = replay(
        &ours,
        &load("shader-compile-stress", SILICON),
        ReplayOptions::default(),
    )
    .unwrap();
    assert!(!report.is_ok(), "a wrong per-tick heap figure must fail");

    // The boot heartbeat, against itself: one digit and the replay is over.
    let boot = load("boot-idle", OURS);
    let corrupt = load_with_body("boot-idle", OURS, |body| {
        body.replace(r#""freeBytes":266688"#, r#""freeBytes":266788"#)
    });
    let report = replay(&corrupt, &boot, ReplayOptions::default()).unwrap();
    assert!(!report.is_ok(), "a wrong freeBytes must fail");
    assert!(
        report.failures().iter().any(|f| f.contains("free_bytes")),
        "{:?}",
        report.failures()
    );
}

/// A transcript of ours replayed against itself is the identity case, and it
/// is worth pinning: it says the parser reads every field it claims to, and
/// that nothing in the boot-idle payload is compared by luck.
#[test]
fn boot_idle_replays_against_itself_field_for_field() {
    let t = load("boot-idle", OURS);
    let again = load("boot-idle", OURS);
    let report = replay(&t, &again, ReplayOptions::default()).unwrap();
    assert!(report.is_ok(), "{:?}", report.failures());
    // 3 memory (free/used/total) + 2 memory (high-water/headroom).
    assert_eq!(report.compared(FieldClass::Memory), 5);
    assert_eq!(report.compared(FieldClass::Wire), 1, "the hello's proto");
    assert!(report.compared(FieldClass::Timing) >= 4);
    assert!(report.compared(FieldClass::Structural) >= 10);
}

/// Strict mode refuses every claim this configuration makes, and that is
/// correct: `lp-emu:esp32c6:t1` is `modeled` in every class in M3, with
/// byte-equality recorded as evidence in the reason rather than as a
/// promotion. When a class is earned, this test is what says so.
#[test]
fn strict_mode_refuses_our_modeled_classes() {
    let report = replay(
        &load("shader-compile-stress", OURS),
        &load("shader-compile-stress", SILICON),
        ReplayOptions {
            strict: true,
            strict_timing: false,
        },
    )
    .unwrap();
    assert!(!report.is_ok());
    let problems = report.grade_problems.join("\n");
    for class in ["memory", "timing"] {
        assert!(
            problems.contains(&format!(
                "configuration `lp-emu:esp32c6:t1` is graded `modeled` for {class}"
            )),
            "{problems}"
        );
    }
    // And the reason travels with the refusal.
    assert!(problems.contains("372/372"), "{problems}");

    let t = load("shader-compile-stress", OURS);
    for class in FieldClass::ALL {
        assert_eq!(
            t.header.trust.grade(*class),
            Grade::Modeled,
            "{class} is graded above modeled in a recorded sidecar"
        );
    }
}

/// Every transcript this milestone recorded is filed where its own header says
/// it should be, carries a sidecar, and names a payload the registry knows.
#[test]
fn the_recorded_transcripts_are_filed_where_their_headers_say() {
    let root = transcripts();
    let mut seen = 0;
    for payload in ["shader-compile-stress", "boot-idle"] {
        let path = root.join(payload).join(OURS);
        let t = Transcript::load(&path).unwrap();
        assert_eq!(t.header.configuration, "lp-emu:esp32c6:t1");
        assert_eq!(t.header.chip, "esp32c6");
        assert_eq!(t.header.payload, payload);
        assert_eq!(
            root.join(t.header.relative_path().unwrap().replace("esp32c6/", "")),
            path
        );
        // The provenance the runner is required to write.
        assert_eq!(t.header.firmware_commit, "d6cfaa2051ae");
        assert_eq!(t.header.firmware_dirty, Some(true));
        assert_eq!(t.header.mac.as_deref(), Some("a0:f2:62:87:b4:8c"));
        assert_eq!(t.header.silicon_rev.as_deref(), Some("v0.2"));
        assert!(t.header.tools.contains_key("lp-emu-esp32c6"));
        assert!(
            t.header.tools["rom"]
                .contains("788e1d38724aeb8fd974fa10c4a7b089c02627d35342ce84b9e0b12b239f3551"),
            "the vendored ROM's sha256 belongs in the sidecar: {:?}",
            t.header.tools
        );
        // The recipe, not a hash of the ELF: the build path is compiled in, so
        // two checkouts of the same source differ in bytes and agree in code.
        let note = t.header.note.as_deref().unwrap_or_default();
        assert!(note.contains("build-reference-image.sh"), "{note}");
        assert!(note.contains("d6cfaa2051ae"), "{note}");
        assert!(t.header.source.as_deref().unwrap().contains("--strict-bus"));
        seen += 1;
    }
    assert_eq!(seen, 2);
}

fn ratio(report: &ReplayReport, scope: &str, field: &str) -> f64 {
    report
        .comparisons
        .iter()
        .find(|c| c.scope == scope && c.field == field)
        .unwrap_or_else(|| panic!("no comparison {scope}.{field}"))
        .ratio
        .unwrap_or_else(|| panic!("{scope}.{field} has no ratio"))
}

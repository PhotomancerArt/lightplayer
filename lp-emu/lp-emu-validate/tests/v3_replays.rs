//! The classic ESP32 (v3), as a replay of committed transcripts.
//!
//! M5 P4 put a board on the desk and captured four payloads off it. This file
//! is the other half of that sitting: for each of those four, an
//! `lp-emu:esp32v3:t1` capture of the **same image bytes**, and the
//! comparison between the two as something the tree can make with no board,
//! no cable and no espflash.
//!
//! # How every transcript here was recorded
//!
//! The images first — one build per payload, in a detached worktree at the
//! commit the silicon sidecars name, which is what makes "the same bytes" a
//! statement rather than a hope:
//!
//! ```bash
//! scripts/emu/build-reference-image.sh --chip esp32 esp32,server,float-f32 c976f17a9 none
//! scripts/emu/build-reference-image.sh --chip esp32 esp32,test_shader_compile_incremental c976f17a9 none
//! scripts/emu/build-reference-image.sh --chip esp32 esp32,test_gpio_calibrate c976f17a9 none
//! scripts/emu/build-reference-image.sh --chip esp32 esp32,test_cycle_probe c976f17a9 none
//! ```
//!
//! then the runs, through `lp-cli validate record` — the front door, never a
//! hand-rolled command line:
//!
//! ```bash
//! cargo run -q -p lp-cli -- validate record v3-boot --config lp-emu:esp32v3:t1 \
//!   --commit c976f17a9 --image target/emu-ref/c976f17a9-boot-idle/fw-esp32v3
//! cargo run -q -p lp-cli -- validate record v3-compile-parity --config lp-emu:esp32v3:t1 \
//!   --commit c976f17a9 \
//!   --image target/emu-ref/c976f17a9-esp32+test_shader_compile_incremental/fw-esp32v3
//! cargo run -q -p lp-cli -- validate record v3-pins --config lp-emu:esp32v3:t1 \
//!   --commit c976f17a9 --timeout-secs 5 \
//!   --image target/emu-ref/c976f17a9-esp32+test_gpio_calibrate/fw-esp32v3
//! cargo run -q -p lp-cli -- validate record cycle-probe --config lp-emu:esp32v3:t1 \
//!   --commit c976f17a9 --image target/emu-ref/c976f17a9-esp32+test_cycle_probe/fw-esp32v3
//! ```
//!
//! `--timeout-secs 5` on `gpio-calibrate` alone: its sentinel is
//! `Sentinel::Ready`, so the run has no `--exit-on` and stops at its deadline,
//! and the default 120 **emulated** seconds of a chip nobody is talking to is
//! two minutes of nothing at a considerable price in wall time.
//!
//! # What the pairing means here
//!
//! **The same bytes, or no comparison.** Each pair below is one ELF, hashed
//! by the recorder into both sidecars, and
//! [`every_pair_ran_one_image`] is the check that the two sha256s are
//! the same string. Without it the difference between two captures could be
//! the difference between two builds, and every number in this file would be
//! measuring the wrong thing (DD30).
//!
//! **The same stimulus.** `boot-idle`'s heartbeat triple is *elicited* on this
//! chip — `esp32_memory_stats` prints it on a stop-all or a `runtime_status`,
//! never from the five-second server heartbeat — so both sides were asked with
//! the same bytes on the same trigger line
//! (`lp-emu/lp-emu-validate/walks/v3-stop-all.script`, which is byte-for-byte
//! `lp-emu-esp32v3/tests/boot_idle.rs`'s `STOP_ALL`).
//!
//! **The same boot.** `espflash` hard-resets after writing, so a board is
//! always captured on the boot *after* the one that formatted `lpfs`. The twin
//! is recorded the same way (`ChipArm::second_boot`), and
//! [`the_ledgers_free_block_is_the_desk_boards`] is what says it worked: a
//! first boot reports `largest_free=106494`, 2032 B short, purely because the
//! format is still live.
//!
//! # What a failure here means
//!
//! A regression, or a re-capture with its own header and its own `-rN` stem.
//! **Never a digit changed in a `.txt`.** The two memory gaps below are
//! pinned to their exact values and a move in *either* direction fails —
//! narrowing one is as much a finding as widening one, and neither is
//! something to tune toward.

use std::collections::HashMap;
use std::path::PathBuf;

use lp_emu_validate::grade::FieldClass;
use lp_emu_validate::replay::{ReplayOptions, ReplayReport, replay};
use lp_emu_validate::transcript::{Transcript, sidecar_path};
use lp_emu_validate::{TranscriptHeader, find_payload};

/// The pinned firmware commit every transcript in this file was built from.
const PIN: &str = "c976f17a9";

/// The desk sitting's captures (M5 P4, PR #699), and our twins beside them.
/// `(payload, silicon, emulated)`.
const PAIRS: &[(&str, &str, &str)] = &[
    (
        "boot-idle",
        "silicon-esp32v3-2026-09-11-c976f17a9-921600.txt",
        "lp-emu-esp32v3-t1-2026-09-11-c976f17a9.txt",
    ),
    (
        "shader-compile-stress",
        "silicon-esp32v3-2026-09-11-c976f17a9-921600.txt",
        "lp-emu-esp32v3-t1-2026-09-11-c976f17a9.txt",
    ),
    (
        "gpio-calibrate",
        "silicon-esp32v3-2026-09-11-c976f17a9-921600.txt",
        "lp-emu-esp32v3-t1-2026-09-11-c976f17a9.txt",
    ),
    (
        "cycle-probe",
        "silicon-esp32v3-2026-09-11-c976f17a9-921600.txt",
        "lp-emu-esp32v3-t1-2026-09-11-c976f17a9.txt",
    ),
];

fn transcripts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32v3")
}

fn load(payload: &str, name: &str) -> Transcript {
    let path = transcripts().join(payload).join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

/// The pair for a payload, `(silicon, emulated)`.
fn pair(payload: &str) -> (Transcript, Transcript) {
    let (_, silicon, ours) = PAIRS
        .iter()
        .find(|(p, _, _)| *p == payload)
        .unwrap_or_else(|| panic!("no pair for `{payload}`"));
    (load(payload, silicon), load(payload, ours))
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

fn diffs(report: &ReplayReport, class: FieldClass) -> Vec<String> {
    report
        .differences_in(class)
        .map(|d| format!("{}.{}: {} vs {}", d.scope, d.field, d.left, d.right))
        .collect()
}

/// The first line of `text` containing `marker`, from `marker` to the end of
/// that line.
///
/// ⚠️ **Not a line-start match, deliberately**, and it is the same reason
/// `lp-emu-esp32v3/tests/boot_idle.rs` reads its fields this way: `esp_println`
/// writes the triple straight into the TX FIFO while `log::info!` lines travel
/// through the io_task's queue, so on BOTH machines a `[stack] heartbeat:` can
/// begin in the middle of somebody else's line. That interleaving is on the
/// wire and the transcripts keep it. Reading from the marker is what makes the
/// figure readable without editing either side — it is not a widened
/// comparison, because the digits it then compares are exact.
fn field_line<'a>(t: &'a Transcript, marker: &str) -> &'a str {
    let line = t
        .lines
        .iter()
        .find(|l| l.contains(marker))
        .unwrap_or_else(|| panic!("no `{marker}` in:\n{}", t.lines.join("\n")));
    let at = line.find(marker).expect("just found it");
    &line[at..]
}

/// The decimal number following `key` in `line`.
fn number(line: &str, key: &str) -> u64 {
    let at = line
        .find(key)
        .unwrap_or_else(|| panic!("no `{key}` in `{line}`"));
    let rest = &line[at + key.len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("`{key}` in `{line}` is not a number"))
}

/// The `[MEM] free=… used=… largest_free=… retry_saves=…` line's four figures.
fn mem_ledger(t: &Transcript) -> HashMap<&'static str, u64> {
    let line = field_line(t, "[MEM] free=");
    ["free=", "used=", "largest_free=", "retry_saves="]
        .into_iter()
        .map(|k| (k, number(line, k)))
        .collect()
}

/// `(high_water, stack_bytes, headroom)` from the first `[stack]` line.
fn stack_report(t: &Transcript) -> (u64, u64, u64) {
    let line = field_line(t, "[stack] heartbeat: high-water ");
    (
        number(line, "high-water "),
        number(line, "B of "),
        number(line, "B ("),
    )
}

// --------------------------------------------------------------------------
// The premise: one image per pair.
// --------------------------------------------------------------------------

/// **The same bytes, stated by both sidecars rather than assumed.**
///
/// Each pair is one ELF: the recorder hashes the image it ran into
/// `firmware_sha256`, silicon's side was hashed the same way at the desk, and
/// this asserts the two strings are equal. It is the premise of every other
/// test in this file — a difference between two builds would otherwise read as
/// a difference between two machines, which is exactly the trap DD30 named.
#[test]
fn every_pair_ran_one_image() {
    for (payload, _, _) in PAIRS {
        let (silicon, ours) = pair(payload);
        assert_eq!(
            silicon.header.configuration, "silicon:esp32v3",
            "{payload}: the right-hand side is the board"
        );
        assert_eq!(
            ours.header.configuration, "lp-emu:esp32v3:t1",
            "{payload}: and the left-hand side is our machine, at t1 (M5 ruling R1: this \
             machine defines no other grade)"
        );
        let a = silicon
            .header
            .firmware_sha256
            .as_deref()
            .unwrap_or_else(|| panic!("{payload}: silicon's sidecar states no image hash"));
        let b = ours
            .header
            .firmware_sha256
            .as_deref()
            .unwrap_or_else(|| panic!("{payload}: our sidecar states no image hash"));
        assert_eq!(a, b, "{payload}: two builds, not two machines");
        assert!(
            silicon.header.firmware_commit.starts_with(PIN)
                && ours.header.firmware_commit.starts_with(PIN),
            "{payload}: {} vs {}",
            silicon.header.firmware_commit,
            ours.header.firmware_commit
        );
        assert_eq!(silicon.header.firmware_dirty, Some(false), "{payload}");
        assert_eq!(ours.header.firmware_dirty, Some(false), "{payload}");
        // And both say they are the same part, so a comparison across chips
        // cannot be filed here by accident.
        assert_eq!(silicon.header.chip, "esp32v3", "{payload}");
        assert_eq!(ours.header.chip, "esp32v3", "{payload}");
    }
}

/// The eFuse identity our machine answers is the desk board's, so a replay
/// compares chip identity rather than a difference in who was told what.
#[test]
fn the_twin_answers_the_desk_boards_identity() {
    for (payload, _, _) in PAIRS {
        let (silicon, ours) = pair(payload);
        assert_eq!(ours.header.mac, silicon.header.mac, "{payload}");
        assert_eq!(
            ours.header.silicon_rev, silicon.header.silicon_rev,
            "{payload}"
        );
        assert_eq!(ours.header.mac.as_deref(), Some("30:76:f5:ec:f6:34"));
        assert_eq!(ours.header.silicon_rev.as_deref(), Some("v3.1"));
    }
}

// --------------------------------------------------------------------------
// `boot-idle` — the shipped image, and the two gaps.
// --------------------------------------------------------------------------

/// **The two machines' ledgers, field by field, and the replay REPORTS the
/// difference rather than absorbing it.**
///
/// Seven of the nine figures in the triple are equal to the byte. Two are not,
/// and both are pinned here to the exact value they have — a move in either
/// direction fails this test, which is what "pinned, not tolerated" means.
///
/// ```text
///                  silicon    emulator    gap
/// [stack] high-water  16972       16492   -480 B
/// [stack] stack       45280       45280      0
/// [stack] headroom    28308       28788   +480 B  (the same gap, other side)
/// [MEM]   free       223352      223268    -84 B
/// [MEM]   used        18200       18284    +84 B
/// [MEM]   largest_free 108526     108526      0
/// [MEM]   retry_saves      0           0      0
/// [JIT]   (nine fields)        identical      0
/// ```
///
/// ⚠️ **The stack gap here is −480 B, and `boot_idle.rs`'s
/// `STACK_HIGH_WATER_GAP` is −640 B.** They are not in conflict and neither is
/// wrong, because **they are not the same measurement**: that constant is a
/// **direct load of the image built at HEAD** (emulator 16332), this is a
/// **ROM-up boot of the image built at `c976f17a9`** (emulator 16492). A stack
/// high-water is the deepest point an interrupt happened to land on, so it
/// moves with the image's layout and with where in the pacer's phase the
/// heartbeat falls. Silicon has no direct load: on a board every boot is a
/// ROM-up boot, so −480 B is the like-for-like figure and the one this phase
/// reports.
///
/// ⚠️ **Corrected by M5 P7.** This paragraph used to bridge the two with
/// `boot_idle.rs`'s cross-path constant — "the ROM-up boot goes 160 B deeper
/// (16332 + 160 = 16492)". **`PATH_HIGH_WATER_GAP` has been −64 since M4 P3b
/// (#704) re-measured it**, and the ROM-up boot goes 64 B *shallower*, not
/// 160 B deeper; re-measured again on 2026-09-11 (direct 16460, rom-up 16396).
/// The bridge is withdrawn. No figure below moved with it.
///
/// `[MEM] used` is +84 B on every path and every quantum measured so far, and
/// the sampling explanation was tested and refuted on both sides. Neither gap
/// is masked, neither threshold is widened, and no mask-set entry hides a
/// memory-class field.
#[test]
fn the_boot_idle_triple_is_stated_field_by_field() {
    let (silicon, ours) = pair("boot-idle");

    let (s_high, s_stack, s_head) = stack_report(&silicon);
    let (o_high, o_stack, o_head) = stack_report(&ours);
    println!("  [stack] silicon  {}", field_line(&silicon, "[stack] "));
    println!("  [stack] emulator {}", field_line(&ours, "[stack] "));
    assert_eq!((s_high, s_stack, s_head), (16_972, 45_280, 28_308));
    assert_eq!((o_high, o_stack, o_head), (16_492, 45_280, 28_788));
    assert_eq!(
        s_stack, o_stack,
        "the stack itself is image-derived and must be identical"
    );
    assert_eq!(
        s_high as i64 - o_high as i64,
        480,
        "the high-water gap moved: {s_high} vs {o_high}. A move in either direction is a \
         finding, never a number to tune"
    );
    assert_eq!(
        s_head as i64 - o_head as i64,
        -480,
        "the same gap, mirrored"
    );

    let s_mem = mem_ledger(&silicon);
    let o_mem = mem_ledger(&ours);
    println!("  [MEM] silicon  {}", field_line(&silicon, "[MEM] "));
    println!("  [MEM] emulator {}", field_line(&ours, "[MEM] "));
    assert_eq!(
        o_mem["used="] as i64 - s_mem["used="] as i64,
        84,
        "`used` gap moved: silicon {} vs emulator {}",
        s_mem["used="],
        o_mem["used="]
    );
    assert_eq!(
        o_mem["free="] as i64 - s_mem["free="] as i64,
        -84,
        "and `free` moves by the same 84 the other way, which is what says the arena is one \
         arena partitioned differently rather than two different arenas"
    );
    assert_eq!(s_mem["used="], 18_200);
    assert_eq!(o_mem["used="], 18_284);
    assert_eq!(s_mem["retry_saves="], o_mem["retry_saves="]);

    // The whole `[JIT]` census, as one string: nine figures, identical.
    assert_eq!(
        field_line(&silicon, "[JIT] used="),
        field_line(&ours, "[JIT] used="),
        "the JIT census is the same on both machines"
    );
    assert_eq!(
        field_line(&ours, "[JIT] used="),
        "[JIT] used=0 peak=0 cap=65536 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 \
         largest_free=65536"
    );
}

/// **`largest_free` is EQUAL, and that is the second boot working.**
///
/// M5 ruling R9: `espflash` hard-resets after writing, so every silicon
/// capture of this payload is the boot *after* the one that formatted the
/// merged image's blank `lpfs`. A machine handed a fresh read-only chip is on
/// its **first** boot and reports `largest_free=106494` — 2032 B short of the
/// board — because the format is still live in the arena. That is a difference
/// between two boots, not between two machines, and absorbing it into a
/// tolerance would have hidden a real number behind a fake one.
///
/// So the twin is recorded over a writable part, run once to format and flush
/// and once for the record, and the figure it produces is the board's to the
/// byte.
#[test]
fn the_ledgers_free_block_is_the_desk_boards() {
    let (silicon, ours) = pair("boot-idle");
    assert_eq!(mem_ledger(&silicon)["largest_free="], 108_526);
    assert_eq!(
        mem_ledger(&ours)["largest_free="],
        108_526,
        "106494 here means the capture was a FIRST boot: the `lpfs` format is still live, and \
         `ChipArm::second_boot` is what stops that happening"
    );
    // And the capture says so in its own words: a second boot mounts.
    let text = ours.lines.join("\n");
    assert!(
        text.contains("[INIT] flash filesystem mounted"),
        "the twin mounted a filesystem"
    );
    assert!(
        !text.contains("Formatted and mounted fresh"),
        "…and did not format one:\n{text}"
    );
}

/// The replay itself: run it, print it, and hold it to exactly two
/// memory-class differences — the stack pair, and nothing else.
///
/// This one **fails** as a replay and that is the correct outcome while the
/// gap stands: `is_ok()` is asserted false on purpose, so a future run in
/// which the gap closes fails this test loudly and has to be re-read rather
/// than passing in silence. The alternative — masking the field, or widening
/// the class — is the lie this plan exists to remove.
#[test]
fn the_boot_idle_replay_reports_the_stack_gap_and_masks_nothing() {
    let (silicon, ours) = pair("boot-idle");
    let report = replay(&ours, &silicon, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());

    let memory = diffs(&report, FieldClass::Memory);
    assert_eq!(memory.len(), 2, "{memory:?}");
    assert!(
        memory
            .iter()
            .any(|d| d.contains("high_water") && d.contains("16492") && d.contains("16972")),
        "{memory:?}"
    );
    assert!(
        memory
            .iter()
            .any(|d| d.contains("headroom") && d.contains("28788") && d.contains("28308")),
        "{memory:?}"
    );
    assert!(
        !report.is_ok(),
        "the gap closed. That is a FINDING, not a passing test: re-read the two figures, \
         re-state them in the trust row's `because`, and tell the director"
    );
    // Everything else this payload compares agrees: the hello frame's wire and
    // structural fields, to the byte.
    assert_eq!(report.differences_in(FieldClass::Wire).count(), 0);
    assert_eq!(report.differences_in(FieldClass::Structural).count(), 0);
    assert!(report.compared(FieldClass::Structural) >= 6, "{report:?}");
}

// --------------------------------------------------------------------------
// The three harness payloads.
// --------------------------------------------------------------------------

/// **Compile parity: 372 memory-class comparisons, every one equal to the
/// byte.**
///
/// The same shader compiled incrementally on both machines, 92 slices, with
/// the heap read either side of each one. That is the claim this payload
/// exists for on this chip, and it is the largest agreement in the classic's
/// record.
///
/// The `timing` class diverges and is **reported, never gated**: `t1` counts
/// one cycle per instruction — no cache, no flash wait states, no LX6
/// per-class cost model — so a slice costs 0.26× what silicon charges for it.
/// There is no `t2` on this machine to calibrate against (M5 ruling R1), which
/// is precisely why a `strict_timing` replay is not what this runs.
#[test]
fn the_compile_harness_agrees_on_every_heap_figure() {
    let (silicon, ours) = pair("shader-compile-stress");
    let report = replay(&ours, &silicon, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());
    assert!(report.is_ok(), "{:?}", report.failures());
    assert_eq!(
        report.differences_in(FieldClass::Memory).count(),
        0,
        "{:?}",
        diffs(&report, FieldClass::Memory)
    );
    // Asserted so a truncated capture cannot pass by having nothing to
    // disagree about.
    assert_eq!(
        report.compared(FieldClass::Memory),
        372,
        "{}",
        report.render()
    );
    assert_eq!(report.differences_in(FieldClass::Structural).count(), 0);
    // And the divergence that IS there is the timing class, out loud.
    assert!(
        report.differences_in(FieldClass::Timing).count() > 0,
        "t1 is not a cycle model and the record should say so"
    );
}

/// The cycle kernels: every structural field agrees, and not one cycle figure
/// is gated.
///
/// `cycle-probe`'s counter is CCOUNT — CPU cycles at 240 MHz — and its figures
/// are RECORDED here rather than compared, for a structural reason and not a
/// squeamish one: `lp-emu:esp32v3` defines `t1` alone, so there is no
/// calibrated grade for such a number to be held to. What these are FOR is
/// being the input a future `t2` would calibrate FROM.
#[test]
fn the_cycle_kernels_agree_on_structure_and_gate_no_cycle() {
    let (silicon, ours) = pair("cycle-probe");
    let report = replay(&ours, &silicon, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());
    assert!(report.is_ok(), "{:?}", report.failures());
    assert_eq!(
        report.differences_in(FieldClass::Structural).count(),
        0,
        "{:?}",
        diffs(&report, FieldClass::Structural)
    );
    assert_eq!(report.compared(FieldClass::Structural), 360);
    // Recorded, and different, and that difference fails nothing.
    assert!(report.differences_in(FieldClass::Timing).count() > 0);
    assert_eq!(report.differences_in(FieldClass::Memory).count(), 0);
    // Both sides ran the kernels to the end.
    for t in [&silicon, &ours] {
        assert!(
            t.lines
                .iter()
                .any(|l| l.contains("[cycle-probe] === DONE ===")),
            "a run that stopped early has nothing to compare"
        );
    }
}

/// The pads' payload reaches its ready line on both machines — and the two
/// sides are **deliberately not conflated**.
///
/// On silicon the firmware drives its own pads and reads them back through
/// `GPIO.in_`, because there is no wire and no hands on that bench. On an
/// emulated configuration the levels would arrive from outside. Both are real;
/// neither is the other, and the payload's own registry entry says so. What
/// this replay compares is therefore the header and the ready line, which is
/// what the two captures genuinely have in common.
#[test]
fn the_pad_harness_reaches_its_ready_line_on_both_machines() {
    let (silicon, ours) = pair("gpio-calibrate");
    for (t, side) in [(&silicon, "silicon"), (&ours, "emulator")] {
        assert!(
            t.lines
                .iter()
                .any(|l| l.contains("CAL READY target=esp32v3")),
            "{side} never got ready"
        );
    }
    let report = replay(&ours, &silicon, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());
    assert!(report.is_ok(), "{:?}", report.failures());
}

// --------------------------------------------------------------------------
// The negative control, and the registry contracts.
// --------------------------------------------------------------------------

/// **A comparison that cannot fail is not a comparison.**
///
/// The file on disk is never touched: the body is edited in memory, reloaded
/// against its own real sidecar, and the replay has to notice. Two edits, one
/// per class, so this control covers the classes the pairs above actually
/// stand on — a heap figure from the compile harness's ledger, and a
/// structural field from the same records.
#[test]
fn an_edited_ledger_fails_the_replay() {
    let (silicon, good) = pair("shader-compile-stress");
    let name = PAIRS
        .iter()
        .find(|(p, _, _)| *p == "shader-compile-stress")
        .unwrap()
        .2;

    // The untouched pair replays clean, so what follows is the edit and not
    // the comparison.
    let clean = replay(&good, &silicon, ReplayOptions::default()).expect("the replay runs");
    assert!(clean.is_ok(), "{:?}", clean.failures());

    // A heap figure moved by eight bytes.
    let mem_before = good
        .lines
        .iter()
        .find_map(|l| l.split("mem_before=").nth(1))
        .and_then(|r| r.split(' ').next())
        .expect("a compile tick's heap reading")
        .to_string();
    let moved: u64 = mem_before
        .trim_end_matches(" free")
        .parse()
        .unwrap_or_else(|_| {
            mem_before
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap()
                .parse()
                .unwrap()
        });
    let liar = load_with_body("shader-compile-stress", name, |body| {
        body.replacen(
            &format!("mem_before={moved}"),
            &format!("mem_before={}", moved + 8),
            1,
        )
    });
    assert_ne!(
        good.lines, liar.lines,
        "the edit has to land or this control proves nothing"
    );
    let report = replay(&liar, &silicon, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());
    assert!(
        report.differences_in(FieldClass::Memory).count() > 0,
        "a moved heap figure has to be caught:\n{}",
        report.render()
    );

    assert!(
        !report.is_ok(),
        "…and it is a FAILURE, not a note:\n{}",
        report.render()
    );

    // And a structural field in a record: which stage the worst slice was in.
    // Deliberately not the series key — a renamed key is a row the other side
    // simply does not have, which is a different thing from a row whose value
    // moved, and only the second of those is what this control is about.
    let liar = load_with_body("shader-compile-stress", name, |body| {
        body.replace(
            r#""max_slice_stage":"Backend""#,
            r#""max_slice_stage":"Frontend""#,
        )
    });
    assert_ne!(good.lines, liar.lines, "the second edit has to land too");
    let report = replay(&liar, &silicon, ReplayOptions::default()).expect("the replay runs");
    assert!(
        report.differences_in(FieldClass::Structural).count() > 0,
        "a moved structural field has to be caught:\n{}",
        report.render()
    );
    assert!(!report.is_ok(), "{}", report.render());
}

/// The classic's own registry contracts, so a later edit to `payload.rs`
/// cannot quietly change what these transcripts are records of.
#[test]
fn the_registry_still_says_what_these_transcripts_are_of() {
    let boot = find_payload("boot-idle").expect("registered");
    let arm = boot.arm("esp32v3").expect("the classic's arm");
    assert!(
        arm.second_boot,
        "M5 ruling R9 — 2032 bytes of `largest_free` hang on it"
    );
    assert_eq!(
        arm.host_script,
        Some("lp-emu/lp-emu-validate/walks/v3-stop-all.script"),
        "the triple is elicited: without the asking there is no capture"
    );
    // The capture ends one line later than the C6's, on the last line of the
    // same triple rather than the first.
    assert_eq!(boot.sentinel_for("esp32v3").marker(), "[JIT] used=");
    assert_eq!(
        boot.sentinel_for("esp32c6").marker(),
        "[stack] heartbeat: high-water",
        "and the C6 is untouched"
    );
    // Every payload here is the SHIPPED classic feature set or a harness
    // build of it, never an emulator-only variant: an emulated twin that ran
    // different features would be measuring a different image.
    for (payload, _, _) in PAIRS {
        let p = find_payload(payload).expect("registered");
        let arm = p.arm("esp32v3").expect("a classic arm");
        assert_eq!(
            arm.emulator_features, None,
            "{payload}: the twin runs the image silicon ran"
        );
        assert!(arm.link.is_uart0(), "{payload}: UART0 is this part's link");
    }
}

/// E1, stated rather than implied: this machine has **one** time grade.
///
/// The C6 has three (`t1` counts instructions, `t2` uses a per-class model,
/// `t3` adds what an access's address costs), and anywhere a reader meets "the
/// C6's three grades" beside the classic's record, the classic's answer is
/// `t1` and only `t1` — there is no measured LX6 cost model to calibrate a
/// second against, and a `t2` that was `t1` under another name is the
/// dishonesty the trust table exists to prevent (M5 ruling R1; the director's
/// E1 answered "t1 only"). A future grade is named future work, not a gap in
/// this file.
#[test]
fn the_classic_defines_one_time_grade() {
    let cfg = lp_emu_validate::config::ValidateConfig::embedded();
    assert!(cfg.configuration("lp-emu:esp32v3:t1").is_ok());
    assert!(
        cfg.configuration("lp-emu:esp32v3:t2").is_err(),
        "a `t2` row would be a claim nothing measured"
    );
    // And the pin CAPABILITY, which is a different claim from the pin GRADE.
    //
    // Our machine drives a pad off its own signal fabric and decodes it back,
    // so it records pins and says so (M5 P7, DD72: `--dump-frames` landed in
    // M4 P3, the first classic `.pins.jsonl` in M4 P5). That does not promote
    // the class — `pin` is still `modeled`, because both readings of the pad
    // are ours and no instrument has been on a classic one.
    //
    // Silicon records NONE, and this is the assertion that keeps it that way:
    // a real pad needs a logic analyser nobody has put on this bench, and a
    // `true` here would turn "we did not look" into "we looked and agreed".
    assert!(cfg.configuration("lp-emu:esp32v3:t1").unwrap().records_pins);
    assert!(!cfg.configuration("silicon:esp32v3").unwrap().records_pins);
}

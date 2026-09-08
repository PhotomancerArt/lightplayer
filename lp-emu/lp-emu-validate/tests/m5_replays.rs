//! M5's gates, as replays of the committed `rmt-chase` and
//! `shader-oracle-walk` transcripts.
//!
//! No firmware, no board, no emulator run: these read what the runner
//! recorded — the console capture, its sidecar, and the **pin capture**
//! beside it — and check the claims the milestone is allowed to make. The
//! machine's own tests (`lp-emu/esp/lp-emu-esp32c6/tests/rmt_chase_replay.rs`,
//! `shader_oracle_pin.rs`) run the image and are `#[ignore]`d for it; these
//! are what makes the gate outlive the sitting.
//!
//! ```bash
//! cargo run -p lp-cli -- validate record emu-m5 --config lp-emu:esp32c6:t1 \
//!   --date 2026-09-07 --commit c0d62e360 --dirty --timeout-secs 20
//! cargo run -p lp-cli -- validate record emu-m5 --config lp-emu:esp32c6:t2 …
//! # the oracle walk alone, at the commit its script and its gate are at:
//! cargo run -p lp-cli -- validate record shader-oracle-walk --config lp-emu:esp32c6:t1 \
//!   --date 2026-09-07 --commit 681ea97ca --timeout-secs 20
//! ```
//!
//! **Never edit a transcript.** A failure here is a regression or a
//! re-capture with its own header, never a digit changed in a `.txt` — or in
//! a `.pins.jsonl`, which is the same rule for the file that says what the
//! wire carried.

use std::path::PathBuf;

use lp_emu_validate::payload::SeriesSpec;
use lp_emu_validate::transcript::Transcript;
use lp_emu_validate::{FieldClass, Payload, ReplayOptions, find_payload, replay};

const T1: &str = "lp-emu-esp32c6-t1-2026-09-07-c0d62e360.txt";
const T2: &str = "lp-emu-esp32c6-t2-2026-09-07-c0d62e360.txt";
const ORACLE_T1: &str = "lp-emu-esp32c6-t1-2026-09-07-681ea97ca.txt";
const ORACLE_T2: &str = "lp-emu-esp32c6-t2-2026-09-07-681ea97ca.txt";
const LEDS: usize = 256;
const FRAMES: usize = LEDS * 3;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32c6/rmt-chase")
}

fn load(file: &str) -> (&'static Payload, Transcript) {
    let path = dir().join(file);
    let t = Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()));
    (find_payload("rmt-chase").unwrap(), t)
}

fn load_oracle(file: &str) -> (&'static Payload, Transcript) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../transcripts/esp32c6/shader-oracle-walk")
        .join(file);
    let t = Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()));
    (find_payload("shader-oracle-walk").unwrap(), t)
}

/// `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture`, run
/// at `7e043ae2d` after `just ci-prereqs`, 2026-09-07 — `[ORACLE] rgb=` and
/// `[ORACLE-RV32] rgb=` were the same 384 characters (`[ORACLE-DIFF] … 0
/// differing bytes of 192`), so one constant stands for both. The frame **as
/// the driver's `write(data)` received it**, RGB order, 64 LEDs;
/// `ORACLE_CRC` is the oracle's own FNV-1a over the same bytes. Restated
/// here rather than read from `lpa-server`, which is outside the `lp-emu/`
/// fence: the number has to be pinned somewhere the replay can reach.
const ORACLE_RGB: &str = "324a0208376a1c2889007668098b4b0375544602631253162b0f7051068a838b000097890b63b208a1601b30951b1c72660069af481900a49554e3212b48e41955cdad4d154f047b103e90441ec10ed47200bcb627f657019fb523c13e3794161c952e04a8743b36e681e90e225ef47f09d1174ebc035c8009447f3fb11b6112ca048dc419dd5fae02903ab21f015c6f026047006a750aa45b69b20834c32b7e8f0012913c086a360144365600567c064d430e9127239632148702475f4c2d05";
const ORACLE_CRC: &str = "0x55772254";

fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash = 0x811c_9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("0x{hash:08x}")
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

fn series(payload: &Payload, name: &str) -> &'static SeriesSpec {
    payload
        .series
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("series `{name}`"))
}

/// FNV-1a over the chase's frame `k`, built from scratch.
///
/// The payload's own definition, restated here as an oracle: pixel `k % 256`
/// is `[10, 10, 10]` and every other pixel is black. Nothing in this crate
/// may import `fw-checks` (the `lp-emu/` fence), which is what makes this an
/// independent check rather than the firmware agreeing with itself.
fn expected_crc(k: usize) -> String {
    let mut frame = vec![0u8; LEDS * 3];
    let at = (k % LEDS) * 3;
    frame[at..at + 3].copy_from_slice(&[10, 10, 10]);
    let mut hash = 0x811c_9dc5u32;
    for byte in &frame {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("0x{hash:08x}")
}

/// **Gate 1.** The recorded run holds the whole chase: 768 records with the
/// checksums the pattern demands, in order, and the sentinel behind them.
#[test]
fn the_chase_is_768_frames_with_the_checksums_the_pattern_demands() {
    let (_, t) = load(T1);
    assert!(
        t.sentinel_line().is_some(),
        "the run reached the done marker"
    );

    let records = t.records_of("rmt-frame").expect("records");
    assert_eq!(records.len(), FRAMES, "three chases of 256");
    for (i, record) in records.iter().enumerate() {
        assert_eq!(
            record.get("n").and_then(|v| v.as_u64()),
            Some(i as u64),
            "record {i} is out of order"
        );
        assert_eq!(record.get("leds").and_then(|v| v.as_u64()), Some(256));
        assert_eq!(record.get("lit").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            record.get("crc").and_then(|v| v.as_str()),
            Some(expected_crc(i).as_str()),
            "frame {i}: the checksum is not the chase's"
        );
    }
    // The chase wraps: frame 256 is frame 0 again, and the payload's own
    // constant says three passes.
    assert_eq!(records[0].get("crc"), records[LEDS].get("crc"));
    assert_eq!(records[0].get("crc"), records[2 * LEDS].get("crc"));
}

/// **Gate 2.** The pin capture: what the pad carried, frame for frame, with
/// the checksum the guest claimed.
#[test]
fn the_pad_carried_every_frame_the_guest_claimed() {
    for file in [T1, T2] {
        let (_, t) = load(file);
        let pins = t.pin_records().expect("the pin capture loads");
        assert_eq!(pins.len(), FRAMES, "{file}: one decoded frame per record");
        for (i, pin) in pins.iter().enumerate() {
            assert_eq!(pin.kind, "ws281x-frame");
            assert_eq!(pin.get("n").and_then(|v| v.as_u64()), Some(i as u64));
            assert_eq!(pin.get("pad").and_then(|v| v.as_u64()), Some(18));
            assert_eq!(
                pin.get("signal").and_then(|v| v.as_str()),
                Some("RMT_SIG_0")
            );
            assert_eq!(pin.get("leds").and_then(|v| v.as_u64()), Some(256));
            assert_eq!(pin.get("bits").and_then(|v| v.as_u64()), Some(6144));
            assert_eq!(pin.get("errors").and_then(|v| v.as_u64()), Some(0));
            assert_eq!(
                pin.get("complete").and_then(|v| v.as_bool()),
                Some(true),
                "{file}: frame {i} was cut short"
            );
            // The bytes the wire carried are the bytes the payload
            // checksummed. `wire`, not `rgb`: `LedChannel` swaps RGB into GRB
            // and `lp-ws281x` permutes again, so the wire carries the
            // caller's frame unswapped and `rgb` is it swapped once more
            // (DD34 d — the double colour swap, filed not fixed). For this
            // payload the two are equal, because a grey dot on black is
            // invariant under any permutation; the assertion is here so that
            // settling the swap later is visible.
            let wire = pin.get("wire").and_then(|v| v.as_str()).expect("wire");
            assert_eq!(pin.get("rgb").and_then(|v| v.as_str()), Some(wire));
            let mut hash = 0x811c_9dc5u32;
            for pair in wire.as_bytes().chunks_exact(2) {
                let byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
                hash ^= u32::from(byte);
                hash = hash.wrapping_mul(0x0100_0193);
            }
            assert_eq!(
                format!("0x{hash:08x}"),
                expected_crc(i),
                "{file}: frame {i} on the wire is not the chase's frame"
            );
        }
    }
}

/// **Gate 3.** The `[WS281X]` line: one per run, and the invariants that
/// hold on any honest configuration.
#[test]
fn the_telemetry_says_nothing_was_truncated_on_either_grade() {
    for file in [T1, T2] {
        let (payload, t) = load(file);
        let samples = t.series(series(payload, "ws281x-telemetry"));
        assert_eq!(samples.len(), 1, "{file}: {samples:?}");
        let v = &samples[0].values;
        assert_eq!(samples[0].key, "0", "{file}: one configured channel");
        assert_eq!(v["half"], "96", "{file}: the one-channel plan's half");
        // The invariants, on each side. They are not cross-configuration
        // equalities — how many frames fit in ten seconds is what a time
        // grade decides — so the replay reports them with their ratio and
        // this is where the claim itself is checked.
        assert_eq!(v["complete"], v["frames"], "{file}: a frame was truncated");
        assert_eq!(v["refills"], v["wanted"], "{file}: a threshold went unfed");
        assert_eq!(v["trips"], "0", "{file}");
        assert_eq!(v["skips"], "0", "{file}");
        assert_eq!(v["errors"], "0", "{file}");
        let frames: u64 = v["frames"].parse().unwrap();
        assert_eq!(
            v["wanted"].parse::<u64>().unwrap(),
            frames * 64,
            "{file}: 64 refills a frame"
        );
    }
}

/// **G3-3.** The two time grades put the same frames on the same pad. Every
/// Pin-class comparison is equal; the timing fields are reported with their
/// ratio and nothing else.
#[test]
fn the_two_time_grades_replay_against_each_other() {
    let (_, a) = load(T1);
    let (_, b) = load(T2);
    let report = replay(&a, &b, ReplayOptions::default()).expect("replay");
    assert!(report.is_ok(), "{}", report.render());

    // 768 frames x 7 pin fields, plus the three telemetry pin fields.
    assert_eq!(report.compared(FieldClass::Pin), FRAMES * 7 + 3);
    assert_eq!(report.differences_in(FieldClass::Pin).count(), 0);
    // 768 records x 4 fields, plus `half`.
    assert_eq!(report.compared(FieldClass::Structural), FRAMES * 4 + 1);

    // …and the clock did move, or the two grades would not be two grades.
    let timing: Vec<_> = report
        .differences_in(FieldClass::Timing)
        .map(|c| format!("{}.{} {} vs {}", c.scope, c.field, c.left, c.right))
        .collect();
    assert!(
        !timing.is_empty(),
        "t1 and t2 should differ somewhere in timing"
    );
    println!(
        "m5_replays: timing differences t1 vs t2:\n  {}",
        timing.join("\n  ")
    );
}

/// The negative control, on the console half: one wrong digit in a frame's
/// claimed checksum has to be visible, or none of the above means anything.
///
/// It fails as a **structural problem** rather than a Pin difference, and the
/// distinction is the point: the guest and the pad of *one* recording
/// disagreeing is not a difference between two configurations, it is a
/// broken recording, and the report says which frame.
#[test]
fn a_corrupted_claim_is_caught_by_the_pad_beside_it() {
    let (_, real) = load(T1);
    let good = expected_crc(500);
    let bad = format!("0x{:08x}", u32::from_str_radix(&good[2..], 16).unwrap() ^ 1);

    // The file on disk is never touched: this reloads the body with one bit
    // of one checksum changed, and points the copy at the same pin capture.
    let mut corrupted = Transcript::from_parts(
        real.header.clone(),
        &real.lines.join("\n").replace(&good, &bad),
    )
    .expect("parses");
    corrupted.path = real.path.clone();

    let claims = corrupted.records_of("rmt-frame").expect("records");
    assert_eq!(claims[500].get("crc").and_then(|v| v.as_str()), Some(&*bad));

    let report = replay(&corrupted, &real, ReplayOptions::default()).expect("replay");
    assert!(
        !report.is_ok(),
        "a changed checksum must fail:\n{}",
        report.render()
    );
    let failures = report.failures().join("\n");
    assert!(
        failures.contains("the guest claims") && failures.contains(&bad),
        "the failure should name the frame and both checksums:\n{failures}"
    );
    // Frame 500 appears three times in the chase (once per pass), so the
    // replacement hits three records — which is itself the pattern's own
    // structure showing through.
    assert!(failures.contains("500"), "{failures}");
}

/// The negative control on the pin half: one wrong byte in what the **wire**
/// carried is a Pin difference, which fails a replay.
#[test]
fn a_corrupted_pin_capture_is_a_pin_difference() {
    let (_, real) = load(T1);
    let pins = std::fs::read_to_string(real.pins_path().expect("a pin capture")).unwrap();

    // A copy in a temp directory, with one nibble of one frame's wire bytes
    // changed. The committed file is never written to.
    let tmp = std::env::temp_dir().join(format!("lp-emu-m5-replays-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let txt = tmp.join(T1);
    std::fs::copy(real.path.as_ref().unwrap(), &txt).unwrap();
    std::fs::copy(
        lp_emu_validate::transcript::sidecar_path(real.path.as_ref().unwrap()),
        lp_emu_validate::transcript::sidecar_path(&txt),
    )
    .unwrap();
    let mut lines: Vec<String> = pins.lines().map(str::to_string).collect();
    let n = 400;
    lines[n] = lines[n].replacen("\"wire\":\"0", "\"wire\":\"1", 1);
    assert_ne!(lines[n], pins.lines().nth(n).unwrap(), "the line changed");
    std::fs::write(
        txt.with_file_name(real.header.pins.clone().unwrap()),
        lines.join("\n"),
    )
    .unwrap();

    let corrupted = Transcript::load(&txt).expect("loads");
    let report = replay(&corrupted, &real, ReplayOptions::default()).expect("replay");
    assert!(
        !report.is_ok(),
        "a changed wire byte must fail:\n{}",
        report.render()
    );
    let pin_diffs: Vec<_> = report
        .differences_in(FieldClass::Pin)
        .map(|c| format!("{}.{}", c.scope, c.field))
        .collect();
    assert_eq!(
        pin_diffs,
        vec![format!("pin[{n}].wire_crc")],
        "exactly one frame's wire should differ"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// **G4-1, outliving the sitting.** The recorded `shader-oracle-walk`: the
/// first lit frame the shipped image put on gpio18 is the host oracle's
/// frame — `rgb` (the wire unpermuted from GRB) equal to `[ORACLE] rgb=`,
/// its FNV-1a the oracle's `crc=` — and every frame after it in the capture
/// is the same bytes. On both time grades.
#[test]
fn the_first_lit_frame_of_the_oracle_walk_is_the_oracles_on_both_grades() {
    assert_eq!(
        fnv1a_hex(&unhex(ORACLE_RGB)),
        ORACLE_CRC,
        "the pinned pair agrees"
    );
    for file in [ORACLE_T1, ORACLE_T2] {
        let (_, t) = load_oracle(file);
        assert!(
            t.sentinel_line().is_some(),
            "{file}: the walk reached the end of projectRead"
        );
        // The console half: the load landed, on the pad the oracle names.
        let text = t.lines.join("\n");
        assert!(text.contains("\"loadProject\":{\"handle\":1}"), "{file}");
        assert!(
            text.contains("gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=192"),
            "{file}: 64 LEDs on D10"
        );
        assert!(
            !text.contains("dropping unparseable"),
            "{file}: the guest lost bytes"
        );

        let pins = t.pin_records().expect("the pin capture loads");
        assert!(!pins.is_empty(), "{file}: no frame on the pad");
        let rgb_of = |r: &lp_emu_validate::Record| {
            r.get("rgb")
                .and_then(|v| v.as_str())
                .expect("rgb")
                .to_string()
        };
        let lit = pins
            .iter()
            .position(|r| rgb_of(r).bytes().any(|c| c != b'0'))
            .unwrap_or_else(|| panic!("{file}: every frame on the pad is black"));
        println!(
            "m5_replays[{file}]: {} frames on the pad, first lit n={lit}, {} after it",
            pins.len(),
            pins.len() - lit - 1
        );
        // Exactly one compile-window black frame before it (ADR
        // 2026-08-03-memory-pressure-at-compile-safe-points); more would be
        // a change in the load path worth knowing about.
        assert_eq!(lit, 1, "{file}: dark frames before the first lit one");
        let first = &pins[lit];
        for (k, v) in [("pad", 18u64), ("leds", 64), ("bits", 1536), ("errors", 0)] {
            assert_eq!(
                first.get(k).and_then(|x| x.as_u64()),
                Some(v),
                "{file}: {k}"
            );
        }
        assert_eq!(
            first.get("signal").and_then(|v| v.as_str()),
            Some("RMT_SIG_0")
        );
        assert_eq!(first.get("complete").and_then(|v| v.as_bool()), Some(true));
        let rgb = rgb_of(first);
        assert_eq!(
            rgb, ORACLE_RGB,
            "{file}: the first lit frame is not the oracle's"
        );
        assert_eq!(fnv1a_hex(&unhex(&rgb)), ORACLE_CRC, "{file}");
        for later in &pins[lit + 1..] {
            // The capture ends on a console line, so its last frame may be
            // the one the run cut; that one is not evidence either way.
            if later.get("complete").and_then(|v| v.as_bool()) != Some(true) {
                assert_eq!(later.get("n"), pins.last().unwrap().get("n"), "{file}");
                continue;
            }
            assert_eq!(
                rgb_of(later),
                ORACLE_RGB,
                "{file}: frame {:?} differs",
                later.get("n")
            );
        }
        assert!(
            pins.len() - lit - 1 >= 5,
            "{file}: too few frames after the first lit one"
        );
    }
}

/// **G4-5.** The two grades' oracle walks replay against each other: every
/// frame the two captures share is the same bytes (`Pin`, equal), the
/// memory-class load gates are equal, and the frame COUNT by the sentinel is
/// reported as timing — the pad ran on at the clock's pace, and the two
/// clocks differ.
#[test]
fn the_oracle_walks_two_grades_replay_against_each_other() {
    let (_, a) = load_oracle(ORACLE_T1);
    let (_, b) = load_oracle(ORACLE_T2);
    let report = replay(&a, &b, ReplayOptions::default()).expect("replay");
    assert!(report.is_ok(), "{}", report.render());
    let shared = a
        .pin_records()
        .unwrap()
        .len()
        .min(b.pin_records().unwrap().len());
    assert_eq!(report.compared(FieldClass::Pin), shared * 7);
    assert_eq!(report.differences_in(FieldClass::Pin).count(), 0);
    assert!(
        report.compared(FieldClass::Memory) > 0,
        "the load gates are in the replay"
    );
    assert_eq!(report.differences_in(FieldClass::Memory).count(), 0);
    let frames: Vec<_> = report
        .comparisons_in(FieldClass::Timing)
        .filter(|c| c.scope == "pin" && c.field == "frames")
        .map(|c| format!("{} vs {}", c.left, c.right))
        .collect();
    assert_eq!(
        frames.len(),
        1,
        "the frame count is reported once, as timing"
    );
    println!("m5_replays: oracle walk pin frames t1 vs t2: {}", frames[0]);
}

/// The negative control on the oracle walk: one nibble of the first lit
/// frame's wire bytes changed in a copy is a `Pin` difference against the
/// committed capture, and the oracle check above would name the frame.
#[test]
fn a_corrupted_oracle_frame_is_a_pin_difference() {
    let (_, real) = load_oracle(ORACLE_T1);
    let pins_text = std::fs::read_to_string(real.pins_path().expect("a pin capture")).unwrap();
    let tmp = std::env::temp_dir().join(format!("lp-emu-m5-oracle-replays-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let txt = tmp.join(ORACLE_T1);
    std::fs::copy(real.path.as_ref().unwrap(), &txt).unwrap();
    std::fs::copy(
        lp_emu_validate::transcript::sidecar_path(real.path.as_ref().unwrap()),
        lp_emu_validate::transcript::sidecar_path(&txt),
    )
    .unwrap();
    let mut lines: Vec<String> = pins_text.lines().map(str::to_string).collect();
    let n = 1;
    lines[n] = lines[n].replacen("\"wire\":\"4", "\"wire\":\"5", 1);
    assert_ne!(
        lines[n],
        pins_text.lines().nth(n).unwrap(),
        "the line changed"
    );
    std::fs::write(
        txt.with_file_name(real.header.pins.clone().unwrap()),
        lines.join("\n"),
    )
    .unwrap();

    let corrupted = Transcript::load(&txt).expect("loads");
    let report = replay(&corrupted, &real, ReplayOptions::default()).expect("replay");
    assert!(
        !report.is_ok(),
        "a changed wire byte must fail:\n{}",
        report.render()
    );
    let pin_diffs: Vec<_> = report
        .differences_in(FieldClass::Pin)
        .map(|c| format!("{}.{}", c.scope, c.field))
        .collect();
    assert_eq!(pin_diffs, vec![format!("pin[{n}].wire_crc")]);
    let _ = std::fs::remove_dir_all(&tmp);
}

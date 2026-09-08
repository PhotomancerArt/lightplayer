//! M5 P3's pin gate: what the guest says it sent, against what the pad
//! carried.
//!
//! The `rmt-chase` payload prints one `rmt-frame` record per frame with an
//! FNV-1a checksum of the RGB bytes it handed the driver. The machine decodes
//! the same frames off GPIO18 with M5 P2's fabric and WS281x decoder, which
//! knows nothing about the guest. This test runs the payload's own image to
//! its sentinel and compares the two, frame by frame — one checksum is a
//! claim, the other is what a logic analyser would have read, and until this
//! phase there was no way to put them side by side.
//!
//! It also reads the transcript through the **host registry**
//! (`lp_emu_validate::Transcript`), not by hand: the record kinds and the
//! `[WS281X]` series pattern that a committed transcript will be replayed
//! with are the ones exercised here, so a registry that cannot parse the
//! firmware's own line fails at `cargo test` rather than at a recording.
//!
//! `#[ignore]`d for the usual reason (`test_support`); `just test-emu-c6`
//! runs it.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use lp_emu_esp_common::strip::ws281x::{Frame, unpermute};
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Outcome, StopCondition, StripConfig, TimeGrade, UsbHost,
};
use lp_emu_esp32c6::periph::rmt::RefillStats;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};
use lp_emu_validate::header::{HEADER_SCHEMA, TranscriptHeader};
use lp_emu_validate::payload::SeriesSpec;
use lp_emu_validate::transcript::Transcript;
use lp_emu_validate::{find_payload, mask_set};

/// The payload's own budget (`Payload::run_secs`), in microseconds. The run
/// ends at the sentinel about 14.9 s in; this is the deadline behind it.
const GATE_US: u64 = 20_000_000;
const PAD: u8 = 18;
const LEDS: usize = 256;
/// `checks::rmt_chase::{LEDS, CHASES}`. Transcribed rather than imported:
/// nothing under `lp-emu/` may depend on `fw-checks` (`just lint-emu-fence`),
/// which is the same fence that makes the payload registry a mirror.
const FRAMES: usize = LEDS * 3;
const DONE: &str = "[rmt-chase] === DONE ===";
/// `ceil(256 * 24 / 96)` threshold events per frame — the refills an
/// untruncated frame needs on the one-channel plan.
const REFILLS_PER_FRAME: u64 = 64;

/// FNV-1a, 32-bit — the fifth transcription of these two constants in the
/// repository and the only one inside the `lp-emu/` fence. See
/// `fw-checks/src/checks/rmt_chase/mod.rs` for the list and the reason; the
/// point of computing it here is that this side must not be able to agree
/// with the guest by sharing its code.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// One run's evidence, kept so the four gates below share two machine runs
/// rather than starting eight. A 14.9 s chase costs about eleven seconds of
/// host time; the tests in this binary are threads in one process, so each
/// grade is run once and read by whichever gate wants it.
#[derive(Clone)]
struct Run {
    capture: String,
    frames: Vec<Frame>,
    refill: RefillStats,
    strip: StripConfig,
    micros: u64,
    instructions: u64,
    exit_matched: bool,
    unmapped: u64,
}

/// `None` when the firmware image is not available and every gate skips.
fn cached(grade: TimeGrade) -> Option<Run> {
    static T1: OnceLock<Option<Run>> = OnceLock::new();
    static T2: OnceLock<Option<Run>> = OnceLock::new();
    let slot = match grade {
        TimeGrade::T1 => &T1,
        _ => &T2,
    };
    slot.get_or_init(|| run(grade)).clone()
}

fn run(grade: TimeGrade) -> Option<Run> {
    let elf = match fw_esp32c6_image(&FwImage::RMT_CHASE) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("rmt_chase_replay", &reason);
            return None;
        }
    };
    println!("rmt_chase_replay: {}", elf.display());
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(grade)
        // The payload's `HostPlan`: a cable and an open port from the first
        // byte. Without a host draining, the records go nowhere — the
        // capture would be the `tried` stream, which is not a transcript.
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the rmt-chase image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US).exit_on(DONE));
    m.flush_frames();
    Some(Run {
        capture: m.usb_sj().text(),
        frames: m.frames(PAD).to_vec(),
        refill: m.rmt_refill_stats(0),
        strip: m.strip(),
        micros: m.micros(),
        instructions: m.instructions(),
        exit_matched: matches!(outcome, Outcome::ExitMatched { .. }),
        unmapped: m.bus.unmapped_reads() + m.bus.unmapped_writes(),
    })
}

/// The `t_ms` the telemetry line stamped itself with, as the cycle window
/// that whole millisecond covers.
///
/// The stamp is `Instant::now().as_millis()`, truncated, and it is read at
/// the top of `report_telemetry_if_due` — after the frame whose completion
/// brought the driver back to the frame-write path. So a frame that ended
/// 300 µs into the stamped millisecond is in the guest's `frames` count and
/// not below `t_ms × 1000` µs on the pad. The claim the two sides can both
/// make is about the millisecond, not about a microsecond inside it.
fn telemetry_window(t: &Transcript) -> (u64, u64) {
    let stamp: u64 = t
        .lines
        .iter()
        .find_map(|l| {
            let at = l.find("[WS281X] t_ms=")? + "[WS281X] t_ms=".len();
            l[at..]
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        })
        .expect("the telemetry line's own stamp");
    let ms = 1_000 * lp_emu_esp32c6::memmap::CYCLES_PER_US;
    (stamp * ms, (stamp + 1) * ms)
}

/// How many frames the pad finished by the start and by the end of the
/// telemetry line's own millisecond.
fn frames_by(frames: &[Frame], (from, to): (u64, u64)) -> (usize, usize) {
    (
        frames.iter().filter(|f| f.end <= from).count(),
        frames.iter().filter(|f| f.end <= to).count(),
    )
}

fn transcript(capture: &str, grade: &str) -> Transcript {
    let header = TranscriptHeader {
        schema: HEADER_SCHEMA,
        payload: "rmt-chase".into(),
        chip: "esp32c6".into(),
        configuration: format!("lp-emu:esp32c6:{grade}"),
        date: "2026-09-07".into(),
        // The image's own account of itself; the loader checks the two
        // halves agree, so this cannot be wrong without failing here.
        firmware_commit: inband_field(capture, "firmware_commit"),
        firmware_features: inband_field(capture, "firmware_features")
            .split(',')
            .map(str::to_string)
            .collect(),
        firmware_dirty: None,
        silicon_rev: None,
        board: None,
        mac: None,
        tools: Default::default(),
        source: None,
        capture: None,
        note: None,
        trust: Default::default(),
    };
    Transcript::from_parts(header, capture).expect("the capture parses as a transcript")
}

/// One string field out of the in-band header line, without pulling a JSON
/// parser in for two fields.
fn inband_field(capture: &str, field: &str) -> String {
    let needle = format!("\"{field}\":\"");
    let at = capture
        .find(&needle)
        .unwrap_or_else(|| panic!("no `{field}` in the in-band header"))
        + needle.len();
    let rest = &capture[at..];
    rest[..rest.find('"').expect("a closing quote")].to_string()
}

fn series(name: &str) -> &'static SeriesSpec {
    find_payload("rmt-chase")
        .expect("registered")
        .series
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("series `{name}`"))
}

fn hist(h: &[u64; 9]) -> String {
    RefillStats::hist_string(h)
}

/// **G3-2.** Every `rmt-frame` record has a decoded frame with the same
/// checksum, and every decoded frame is complete.
#[test]
#[ignore = "needs the fw-esp32c6 rmt-chase ELF; run through `just test-emu-c6`"]
fn every_frame_the_guest_claims_is_on_the_pad_with_the_same_checksum() {
    let Some(r) = cached(TimeGrade::T1) else {
        return;
    };
    assert!(
        r.exit_matched,
        "the run should end at the payload's sentinel"
    );
    assert_eq!(r.unmapped, 0, "unmapped");

    let t = transcript(&r.capture, "t1");
    assert!(t.sentinel_line().is_some(), "the done marker is in the run");
    let records = t.records_of("rmt-frame").expect("records parse");
    println!(
        "rmt_chase_replay: {} records, {} decoded frames on gpio{PAD}, \
         {} us emulated, {} instructions",
        records.len(),
        r.frames.len(),
        r.micros,
        r.instructions,
    );
    assert_eq!(records.len(), FRAMES, "one record per frame of the chase");
    assert_eq!(r.frames.len(), FRAMES, "one decoded frame per record");

    for (i, (record, frame)) in records.iter().zip(r.frames.iter()).enumerate() {
        let n = record.get("n").and_then(|v| v.as_u64()).expect("n");
        assert_eq!(n as usize, i, "record {i} is out of order");
        assert_eq!(frame.n, i as u64, "decoded frame {i} is out of order");
        assert_eq!(record.get("leds").and_then(|v| v.as_u64()), Some(256));
        assert_eq!(record.get("lit").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(frame.leds(), LEDS, "frame {i}");
        assert_eq!(frame.bits, LEDS * 24, "frame {i}");
        assert_eq!(frame.error_count, 0, "frame {i}");
        assert!(frame.is_complete(), "frame {i} was cut short: {frame:?}");

        let claimed = record.get("crc").and_then(|v| v.as_str()).expect("crc");
        let observed = format!("0x{:08x}", fnv1a(&frame.wire));
        assert_eq!(
            claimed, observed,
            "frame {i}: the guest claims {claimed}, the pad carried {observed}"
        );
    }

    // The chase itself, read off the wire: pixel `n % 256` is the white dot
    // and everything else is black. Checksums agreeing would not say that.
    for (i, frame) in r.frames.iter().enumerate() {
        for (px, pixel) in frame.wire.chunks_exact(3).enumerate() {
            let expected: [u8; 3] = if px == i % LEDS {
                [10, 10, 10]
            } else {
                [0, 0, 0]
            };
            assert_eq!(pixel, expected, "frame {i}, pixel {px}");
        }
        // White on black is invariant under any channel permutation, which is
        // why the payload's claim survives the harness's double colour swap
        // (`LedChannel` swaps RGB→GRB and `lp-ws281x` permutes again, so the
        // wire carries the caller's RGB). Stated as an assertion so that
        // settling the swap later shows up here rather than in a checksum.
        assert_eq!(
            unpermute(&frame.wire, r.strip.order),
            frame.wire,
            "frame {i}: the chase should be colour-order invariant"
        );
    }
}

/// **G3-1's telemetry half.** The `[WS281X]` line the payload prints, read
/// through the registry's own series, and the Pin-class claims in it checked
/// against the pad.
#[test]
#[ignore = "needs the fw-esp32c6 rmt-chase ELF; run through `just test-emu-c6`"]
fn the_telemetry_line_agrees_with_the_pad_and_the_histograms_are_reported() {
    let Some(r) = cached(TimeGrade::T1) else {
        return;
    };
    let t = transcript(&r.capture, "t1");
    let samples = t.series(series("ws281x-telemetry"));
    assert_eq!(
        samples.len(),
        1,
        "three chases cross the ten-second reporting period exactly once: {samples:?}"
    );
    let v: &BTreeMap<String, String> = &samples[0].values;
    println!("rmt_chase_replay: [WS281X] {v:?}");
    assert_eq!(samples[0].key, "0", "one configured channel");
    assert_eq!(v["half"], "96", "the one-channel plan's half-window");

    let frames: u64 = v["frames"].parse().expect("frames");
    assert_eq!(v["complete"], v["frames"], "no frame was truncated");
    assert_eq!(v["trips"], "0");
    assert_eq!(v["skips"], "0");
    assert_eq!(v["errors"], "0");
    assert_eq!(v["refills"], v["wanted"], "every threshold was answered");
    assert_eq!(
        v["wanted"].parse::<u64>().expect("wanted"),
        frames * REFILLS_PER_FRAME,
        "64 refills a frame"
    );

    // The claim the Pin class is for: `frames` is what reached the wire, and
    // the wire says the same. The telemetry line is printed at its own
    // `t_ms`, so the pad is asked the same question — how many frames had
    // finished by then.
    let (least, most) = frames_by(&r.frames, telemetry_window(&t));
    assert!(
        (least..=most).contains(&(frames as usize)),
        "the guest counted {frames} frames at its stamp; the pad finished {least} by the \
         start of that millisecond and {most} by its end"
    );

    // Reported, never gated (D13/PD9): the emulator's own reading of the same
    // race, in the same units and the same buckets.
    let s = r.refill;
    println!(
        "rmt_chase_replay: emulator refill ch0: {} measured, half={}, \
         entry max {} hist {}, fill max {} hist {}, {} unanswered\n\
         rmt_chase_replay: guest lag_avg={} lag_max={} hist={} entry_max={} entry_hist={}",
        s.refills,
        s.half_words,
        s.entry_max,
        hist(&s.entry_hist),
        s.fill_max,
        hist(&s.fill_hist),
        s.unanswered,
        v["lag_avg"],
        v["lag_max"],
        v["hist"],
        v["entry_max"],
        v["entry_hist"],
    );
    // Every threshold of every frame was answered by the ISR, the last one of
    // a frame included — `refill` flips `tx_lim` before it discovers the
    // frame has drained — so nothing is unanswered and the emulator's count
    // is the guest's `wanted` for the whole run rather than for the ten
    // seconds the telemetry line covers.
    assert_eq!(s.refills, FRAMES as u64 * REFILLS_PER_FRAME);
    assert_eq!(s.unanswered, 0);
    assert_eq!(s.half_words, 96);
}

/// The mask set leaves every counter comparable and touches only the
/// telemetry line's own clock. A mask that erased `frames=` would erase the
/// gate, so this is checked rather than assumed.
#[test]
#[ignore = "needs the fw-esp32c6 rmt-chase ELF; run through `just test-emu-c6`"]
fn the_mask_set_hides_the_clock_and_nothing_else() {
    let Some(r) = cached(TimeGrade::T1) else {
        return;
    };
    let t = transcript(&r.capture, "t1");
    let set = mask_set("rmt-chase").expect("the payload's mask set");
    let masked = t.masked(set).join("\n");
    assert!(masked.contains("[WS281X] t_ms=N "), "the stamp is masked");

    // The counters, whatever they are: taken from the raw line so the
    // assertion is "the mask did not touch this", not a second copy of the
    // figures the gate above already checks.
    let raw = t
        .lines
        .iter()
        .find(|l| l.contains("[WS281X] t_ms="))
        .expect("the telemetry line");
    let counters = &raw[raw.find(" ch=").expect("ch=")..];
    assert!(
        masked.contains(counters),
        "the mask changed a counter:\n  raw    {counters}\n  masked {masked}"
    );

    // And the per-frame checksums: frame 0's is a fixed function of the
    // pattern, so it can be named.
    assert!(
        masked.contains(r#""crc":"0xf88210a7""#),
        "the mask changed frame 0's checksum"
    );
}

/// **G3-3.** `t2` transmits the same frames: identical records, identical
/// pin checksums, identical Pin-class telemetry. Only the clock moves.
#[test]
#[ignore = "needs the fw-esp32c6 rmt-chase ELF; run through `just test-emu-c6`"]
fn the_second_time_grade_puts_the_same_frames_on_the_pad() {
    let (Some(a), Some(b)) = (cached(TimeGrade::T1), cached(TimeGrade::T2)) else {
        return;
    };

    let (ta, tb) = (transcript(&a.capture, "t1"), transcript(&b.capture, "t2"));
    let (ra, rb) = (
        ta.records_of("rmt-frame").expect("t1 records"),
        tb.records_of("rmt-frame").expect("t2 records"),
    );
    assert_eq!(ra.len(), FRAMES);
    assert_eq!(rb.len(), FRAMES);
    for (x, y) in ra.iter().zip(rb.iter()) {
        assert_eq!(x.get("n"), y.get("n"));
        assert_eq!(x.get("crc"), y.get("crc"), "record {:?}", x.get("n"));
    }

    assert_eq!(
        a.frames.len(),
        b.frames.len(),
        "the same number of frames on the pad"
    );
    for (x, y) in a.frames.iter().zip(b.frames.iter()) {
        assert_eq!(x.wire, y.wire, "frame {} differs on the wire", x.n);
        assert_eq!(x.is_complete(), y.is_complete(), "frame {}", x.n);
    }

    let spec = series("ws281x-telemetry");
    let (sa, sb) = (ta.series(spec), tb.series(spec));
    assert_eq!(sa.len(), 1);
    assert_eq!(sb.len(), 1);
    for field in ["half", "trips", "skips", "errors"] {
        assert_eq!(sa[0].values[field], sb[0].values[field], "field `{field}`");
    }
    // `frames` and `complete` are NOT asserted equal across the grades: the
    // line is printed at ten seconds of **guest** time and the two grades do
    // not put the same number of frames in ten seconds, which is the whole
    // reason a time grade exists. What must hold on both sides is that the
    // count agrees with the pad, and that nothing was truncated.
    println!(
        "rmt_chase_replay: t1 frames={} in {} us / {} instructions, \
         t2 frames={} in {} us / {} instructions (guest-time ratio {:.3}x)",
        sa[0].values["frames"],
        a.micros,
        a.instructions,
        sb[0].values["frames"],
        b.micros,
        b.instructions,
        b.micros as f64 / a.micros as f64,
    );
    for (t, run, side) in [(&ta, &a, "t1"), (&tb, &b, "t2")] {
        let s = t.series(spec);
        let frames: usize = s[0].values["frames"].parse().expect("frames");
        assert_eq!(s[0].values["complete"], s[0].values["frames"], "{side}");
        let (least, most) = frames_by(&run.frames, telemetry_window(t));
        assert!(
            (least..=most).contains(&frames),
            "{side}: the guest counted {frames}; the pad finished {least}..={most} \
             inside the stamped millisecond"
        );
    }
}

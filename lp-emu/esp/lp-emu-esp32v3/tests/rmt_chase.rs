//! **M4 P5's gate**: what the guest says it sent, against what the pad
//! carried — on the classic, frame for frame, for all 768.
//!
//! The `rmt-chase` payload (`fw-esp32v3 --features esp32,test_rmt`) prints one
//! `rmt-frame` record per frame carrying an FNV-1a checksum of the RGB bytes
//! it handed the driver. The machine decodes the same frames off **IO18** with
//! M4 P3's fabric and WS281x decoder, which knows nothing about the guest, and
//! computes the same checksum from the waveform. The gate is that the two
//! agree on every frame.
//!
//! ⚠️ **Both readings are ours.** The decoder is this repository's, the fabric
//! is this repository's and the RMT model is this repository's, so this is two
//! readings of one machine and not a measurement. What it buys is that a bug
//! now has to be in the same place in three independent code paths to hide.
//! **The silicon twin of this transcript is M5's**, and it is the only thing
//! that turns any of it into a measurement.
//!
//! # What is new here against `pin_frames.rs`
//!
//! P3's waveform was driven **from the host**: the test wrote the shipped
//! image's own register sequence through the bus and ran the firmware's
//! refill. This is the first **guest-driven** channel on the classic — the
//! firmware opens it, starts every frame, and services every refill from its
//! own ISR — and the first comparison of a frame against something outside the
//! machine's own decoder, namely the guest's arithmetic over its own buffer.
//!
//! # Cost, and what runs at full length
//!
//! The payload is 768 frames at ≈ 18.07 ms each: **13.885 s of guest time,
//! 3.33 billion cycles, about 107 s of host time per run.** So the checksum
//! gate runs the payload whole — that is the claim — and the determinism and
//! snapshot gates run a bounded **prefix** of the same run instead of paying
//! for two more. Both are properties of the decode path per frame, not of the
//! 768th frame in particular, and `tests/determinism.rs` and
//! `tests/pin_frames.rs` already hold the machine to the same rules on the
//! shipped image. The prefix is stated in cycles and named in each test.
//!
//! `#[ignore]`d for the reason `test_support`'s module docs give — a plain
//! `cargo test --workspace` must never start a cross-target firmware build.
//! `just test-emu-esp32v3-boot` builds the harness image, names the file it
//! built, and runs this.

use std::sync::OnceLock;

use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::{Frame, unpermute};
use lp_emu_esp32v3::machine::{
    AppSource, Esp32V3Builder, FrameSink, Machine, Outcome, StopCondition, StripConfig, TimeGrade,
};
use lp_emu_esp32v3::periph::rmt;
use lp_emu_esp32v3::test_support::{fw_esp32v3_test_rmt_image, skip_notice};
use sha2::{Digest, Sha256};

/// The pad: the DOM-Z-102's "data channel 1", the wire M4's walk uses, and the
/// number the C6's own `rmt-chase` harness drives.
const PAD: u8 = 18;

/// `checks::rmt_chase::LEDS`. Transcribed rather than imported: nothing under
/// `lp-emu/` may depend on `fw-checks` (`just lint-emu-fence`), which is the
/// same fence that keeps this side from agreeing with the guest by sharing its
/// code.
const LEDS: usize = 256;
/// `checks::rmt_chase::{LEDS, CHASES}` — three passes of the dot.
const FRAMES: usize = LEDS * 3;
/// `checks::rmt_chase::DOT`, the lit pixel. White, so the pattern says the
/// same thing under any permutation of the three channels.
const DOT: [u8; 3] = [10, 10, 10];
/// `checks::rmt_chase::DONE_MARKER`.
const DONE: &str = "[rmt-chase] === DONE ===";

/// The deadline behind the sentinel. The payload ends at ≈ 13.885 s; this is
/// what stops a run whose sentinel never arrives.
const GATE_US: u64 = 20_000_000;

/// The prefix the determinism and snapshot gates run, in emulated
/// microseconds. **Measured, not assumed**: on a clean-tree build of this
/// image the first frame starts at 8,836.046 µs and runs 7,679.150 µs, and
/// the period is 18,067 µs — so 60 ms holds three frames and the start of a
/// fourth.
const PREFIX_US: u64 = 60_000;

/// A cycle inside the **first** frame, for the snapshot gate: between its
/// start at 8,836.046 µs and its end at 16,515.196 µs.
///
/// ⚠️ **The assertion in that test is what proves the point, not this
/// arithmetic** — and it has to be, because the start moves with the *length*
/// of what the guest prints before it. A dirty-tree build stamps
/// `"firmware_dirty":true` into the in-band header, one byte shorter than
/// `false`, and the first frame then starts one UART symbol (2,610 cycles,
/// 10.875 µs) earlier. That is the UART drain being modelled at baud in
/// emulated time, and it is why no gate here is a microsecond.
const MID_FRAME_US: u64 = 12_000;

/// Host-side safety net, so a wedged run fails the suite instead of hanging
/// it. Generous against the ≈ 107 s a full run costs.
const WALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);

/// FNV-1a, 32-bit. The **sixth** transcription of these two constants in the
/// repository and the second inside the `lp-emu/` fence (the C6's
/// `rmt_chase_replay.rs` is the other); see
/// `fw-checks/src/checks/rmt_chase/mod.rs` for the list and the reason. The
/// whole point of computing it here is that this side must not be able to
/// agree with the guest by sharing its code.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// One `rmt-frame` record, as the guest printed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Record {
    n: usize,
    leds: usize,
    lit: usize,
    crc: u32,
}

/// The records in a capture, in the order the guest printed them.
///
/// Hand-parsed rather than read through `lp-emu-validate`'s registry: wiring
/// this payload into the validation system is **M5's** (the phase's
/// out-of-scope line), and a gate that needed the registry to exist would have
/// smuggled it in.
fn records(capture: &str) -> Vec<Record> {
    capture
        .lines()
        .filter(|l| l.contains(r#""kind":"rmt-frame""#))
        .map(|line| Record {
            n: field(line, "n"),
            leds: field(line, "leds"),
            lit: field(line, "lit"),
            crc: {
                let text = quoted(line, "crc");
                let hex = text.strip_prefix("0x").unwrap_or(&text);
                u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("crc in {line}"))
            },
        })
        .collect()
}

fn field(line: &str, name: &str) -> usize {
    let needle = format!(r#""{name}":"#);
    let at = line
        .find(&needle)
        .unwrap_or_else(|| panic!("no `{name}` in {line}"))
        + needle.len();
    line[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("`{name}` is not a number in {line}"))
}

fn quoted(line: &str, name: &str) -> String {
    let needle = format!(r#""{name}":""#);
    let at = line
        .find(&needle)
        .unwrap_or_else(|| panic!("no `{name}` in {line}"))
        + needle.len();
    let rest = &line[at..];
    rest[..rest.find('"').expect("a closing quote")].to_string()
}

/// One run's evidence, so the gates below share **one** 107-second run rather
/// than starting one each. The tests in a binary are threads in one process.
#[derive(Clone)]
struct Run {
    capture: String,
    frames: Vec<Frame>,
    routed: Vec<(PadId, RouteSource)>,
    strip: StripConfig,
    micros: u64,
    cycles: u64,
    instructions: u64,
    idle_skips: u64,
    quantum: u64,
    edges: u64,
    dropped_edges: u64,
    unmapped: u64,
    exit_matched: bool,
}

/// A machine on the harness image, or `None` when the image is not available
/// and every gate skips.
fn machine(test: &str, dump: FrameSink) -> Option<Machine> {
    let elf = match fw_esp32v3_test_rmt_image() {
        Ok(path) => path,
        Err(reason) => {
            skip_notice(test, &reason);
            return None;
        }
    };
    println!("{test}: {}", elf.display());
    Some(
        Esp32V3Builder::new()
            .app(AppSource::Path(elf))
            // Strict for every gate run (M4 notes §11): an address this
            // machine does not model is a finding, not something to permit.
            .strict(true)
            .time_grade(TimeGrade::T1)
            .dump_frames(dump)
            .build()
            .expect("the test_rmt image builds a machine"),
    )
}

/// The whole payload, once, cached.
fn full_run() -> Option<Run> {
    static RUN: OnceLock<Option<Run>> = OnceLock::new();
    RUN.get_or_init(|| {
        let mut m = machine("rmt_chase", FrameSink::Memory)?;
        let stop = StopCondition {
            stop_cycle: Some(GATE_US * lp_emu_esp32v3::memmap::CYCLES_PER_US),
            exit_on: Some(DONE.to_string()),
            wall_timeout: Some(WALL_TIMEOUT),
            probes: Vec::new(),
        };
        let outcome = m.run_until(&stop);
        // ⚠️ A frame is not closed until something follows its latch: the
        // decoder reports an open frame *incomplete* rather than inventing a
        // reset gap. Flush before reading the last of the 768.
        m.flush_frames();
        Some(Run {
            capture: m.uart0().text(),
            frames: m.frames(PAD).to_vec(),
            routed: m.routed_pads(),
            strip: m.strip(),
            micros: m.micros(),
            cycles: m.cycles(),
            instructions: m.instructions(),
            idle_skips: m.idle_skips(),
            quantum: m.core_quantum(),
            edges: m.pin_edges(PAD),
            dropped_edges: m.bus().pins.dropped_edges(),
            unmapped: m.bus().unmapped_reads() + m.bus().unmapped_writes(),
            exit_matched: matches!(outcome, Outcome::ExitMatched { .. }),
        })
    })
    .clone()
}

// ---------------------------------------------------------------------------
// 1. The routing
// ---------------------------------------------------------------------------

/// **The pad the frames came off is IO18's, and it is `RMT_SIG_0`.**
///
/// ⚠️ This is deliberately **not** "exactly one routing note". M4 P3 found
/// that a plain boot of any classic image routes **gpio1** too — the console
/// TX pad, through `func_out_sel_cfg` — so that pad gets a decoder and a
/// summary line of its own, and asserting the list's length would fail on a
/// fact about the console rather than about the chase. What is asserted
/// instead is IO18's route by value, and that **no other RMT signal** is
/// routed anywhere: a second transmitter appearing on a pad is the thing that
/// would actually corrupt this gate.
#[test]
#[ignore = "needs the fw-esp32v3 test_rmt ELF; run through `just test-emu-esp32v3-boot`"]
fn io18_carries_rmt_sig_0_and_nothing_else_carries_an_rmt_signal() {
    let Some(r) = full_run() else {
        return;
    };
    let mine = r
        .routed
        .iter()
        .find(|(pad, _)| *pad == PadId(PAD))
        .unwrap_or_else(|| panic!("gpio{PAD} is not routed: {:?}", r.routed));
    assert_eq!(
        mine.1,
        RouteSource::Signal(SignalId(rmt::RMT_SIG_0), false),
        "gpio{PAD} should carry RMT_SIG_0 (out_sel=87), not inverted"
    );
    let rmt_signals: Vec<&(PadId, RouteSource)> = r
        .routed
        .iter()
        .filter(|(_, source)| match source {
            RouteSource::Signal(SignalId(id), _) => {
                (rmt::RMT_SIG_0..rmt::RMT_SIG_0 + 8).contains(id)
            }
            RouteSource::GpioOut => false,
        })
        .collect();
    assert_eq!(
        rmt_signals.len(),
        1,
        "one RMT signal is routed and it is gpio{PAD}'s: {rmt_signals:?}"
    );
    println!("rmt_chase: routed pads {:?}", r.routed);
}

// ---------------------------------------------------------------------------
// 2. The gate: 768 frames, checksum for checksum
// ---------------------------------------------------------------------------

/// **Every frame the decoder read off IO18 is checksum-equal to the guest's
/// own record**, with zero bit errors, a reset gap between frames, and the
/// chase pixel where the record says it is.
///
/// The two checksums are computed from different things by different code:
/// the guest's over the buffer it handed the driver, this one over the bytes
/// the pad carried, unpermuted back out of the wire's colour order.
///
/// ⚠️ `unpermute`, not the wire bytes directly. The C6's twin compares
/// `fnv1a(&frame.wire)` because `LedChannel` swaps RGB→GRB and then
/// `lp-ws281x` permutes again, so the C6's wire carries the caller's RGB
/// (`fw-checks`'s "double swap", DD34 d). The classic has **no such wrapper**
/// — the harness hands `Ws281xDriver::send_blocking` the caller's RGB and
/// `lp-ws281x` permutes once — so the wire here carries real GRB and the
/// unpermutation is what makes the comparison right. This payload cannot tell
/// the difference on its own (every pixel is grey, so the two are equal byte
/// for byte on all 768 frames), which is exactly why the correct form is
/// written here rather than the one that happens to pass.
#[test]
#[ignore = "needs the fw-esp32v3 test_rmt ELF; run through `just test-emu-esp32v3-boot`"]
fn every_frame_the_guest_claims_is_on_the_pad_with_the_same_checksum() {
    let Some(r) = full_run() else {
        return;
    };
    assert!(
        r.exit_matched,
        "the run should end at the payload's own done marker"
    );
    assert_eq!(r.unmapped, 0, "unmapped bus access");
    assert_eq!(r.dropped_edges, 0, "edges were dropped on the way to a pad");
    assert!(
        r.capture.contains(r#""payload":"rmt-chase""#),
        "the in-band payload header is missing from the capture"
    );

    let records = records(&r.capture);
    println!(
        "rmt_chase: {} records, {} decoded frames on gpio{PAD}, {} edges, \
         {} us emulated, {} cycles, {} instructions, quantum {}, idle skips {}",
        records.len(),
        r.frames.len(),
        r.edges,
        r.micros,
        r.cycles,
        r.instructions,
        r.quantum,
        r.idle_skips,
    );
    assert_eq!(records.len(), FRAMES, "one record per frame of the chase");
    assert_eq!(r.frames.len(), FRAMES, "one decoded frame per record");

    let order = r.strip.order;
    let mut equal = 0usize;
    let mut bit_errors = 0u64;
    for (i, (record, frame)) in records.iter().zip(r.frames.iter()).enumerate() {
        assert_eq!(record.n, i, "record {i} is out of order");
        assert_eq!(record.leds, LEDS, "record {i}");
        assert_eq!(record.lit, 1, "record {i}");
        assert_eq!(frame.n, i as u64, "decoded frame {i} is out of order");
        assert_eq!(frame.pad, PadId(PAD), "frame {i}");
        assert_eq!(frame.bits, LEDS * 24, "frame {i}: 6,144 bits");
        assert_eq!(frame.leds(), LEDS, "frame {i}");
        assert_eq!(frame.trailing_bits, 0, "frame {i}");
        assert_eq!(frame.error_count, 0, "frame {i}: {:?}", frame.errors);
        assert!(frame.is_complete(), "frame {i} was cut short: {frame:?}");
        // The reset gap the decoder saw close this frame. Present, and
        // reported — never a microsecond gate (PD9).
        assert!(
            frame.reset_cycles.is_some(),
            "frame {i} closed without a reset gap"
        );
        bit_errors += frame.error_count;

        let rgb = unpermute(&frame.wire, order);
        let observed = fnv1a(&rgb);
        assert_eq!(
            observed, record.crc,
            "frame {i}: the guest claims 0x{:08x}, the pad carried 0x{observed:08x}",
            record.crc
        );
        equal += 1;

        // …and the chase is where the record says: pixel `i % 256` lit, the
        // rest black. The checksum alone would pass on a frame that was
        // consistently wrong in both readings.
        for (pixel, px) in rgb.chunks_exact(3).enumerate() {
            let expected = if pixel == i % LEDS { DOT } else { [0, 0, 0] };
            assert_eq!(px, expected, "frame {i}, pixel {pixel}");
        }
    }
    println!("rmt_chase: {equal} of {FRAMES} frames checksum-equal, {bit_errors} bit errors");
    assert_eq!(equal, FRAMES);
    assert_eq!(bit_errors, 0);

    // The gaps, reported as a shape rather than gated: the 300 µs latch plus
    // the harness's own 10 ms `Delay`, and a frame period of transmission plus
    // that gap. Both are the *guest's* waits, which is why the assertion is
    // "present and the same for every frame", not a number of microseconds.
    let resets: Vec<u64> = r.frames.iter().filter_map(|f| f.reset_cycles).collect();
    assert_eq!(resets.len(), FRAMES, "every frame closed on a reset");
    let cycles_per_us = lp_emu_esp32v3::memmap::CYCLES_PER_US as f64;
    let (reset_min, reset_max) = (
        *resets.iter().min().expect("frames") as f64 / cycles_per_us,
        *resets.iter().max().expect("frames") as f64 / cycles_per_us,
    );
    let periods: Vec<u64> = r
        .frames
        .windows(2)
        .map(|w| w[1].start - w[0].start)
        .collect();
    let (p_min, p_max) = (
        *periods.iter().min().expect("frames") as f64 / cycles_per_us,
        *periods.iter().max().expect("frames") as f64 / cycles_per_us,
    );
    println!(
        "rmt_chase: reset {reset_min:.1}..{reset_max:.1} us between frames, \
         frame period {p_min:.1}..{p_max:.1} us"
    );

    // Two edges per bit — a rise and a fall — and nothing else. The latch's
    // low follows the last bit's low, and the pad rests at the level it is
    // already at, so a whole run of complete frames is exact.
    let bits: usize = r.frames.iter().map(|f| f.bits).sum();
    assert_eq!(
        r.edges as usize,
        2 * bits,
        "{} edges for {bits} bits",
        r.edges
    );
}

// ---------------------------------------------------------------------------
// 3. Determinism, and a decoder caught mid-bit
// ---------------------------------------------------------------------------

/// **Two runs write byte-identical `--dump-frames` files.**
///
/// A bounded prefix ([`PREFIX_US`]) rather than the whole payload — see the
/// module docs on cost. What is being pinned is that the decode path is a pure
/// function of the instruction stream, and three frames of it is the same
/// claim as 768 at seventy times the price.
#[test]
#[ignore = "needs the fw-esp32v3 test_rmt ELF; run through `just test-emu-esp32v3-boot`"]
fn two_runs_dump_identical_frames() {
    let dir = std::env::temp_dir();
    let paths = [
        dir.join("lp-emu-esp32v3-m4-p5-frames-a.jsonl"),
        dir.join("lp-emu-esp32v3-m4-p5-frames-b.jsonl"),
    ];
    let mut digests = Vec::new();
    for path in &paths {
        let Some(mut m) = machine("rmt_chase", FrameSink::File(path.clone())) else {
            return;
        };
        let outcome = m.run_until(&StopCondition::after_micros(PREFIX_US));
        assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
        m.flush_frames();
        drop(m);
        let bytes = std::fs::read(path).expect("the dump file");
        assert!(bytes.len() > 1_000, "{} bytes", bytes.len());
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        digests.push((format!("{:x}", hasher.finalize()), bytes));
    }
    println!(
        "rmt_chase: --dump-frames sha256 {} and {}",
        digests[0].0, digests[1].0
    );
    assert_eq!(
        digests[0].0, digests[1].0,
        "two runs dumped different frames"
    );
    assert_eq!(digests[0].1, digests[1].1);
    let first = String::from_utf8(digests[0].1.clone()).expect("utf-8");
    let line = first.lines().next().expect("a record");
    println!("rmt_chase: first record {}", &line[..line.len().min(200)]);
    for path in &paths {
        let _ = std::fs::remove_file(path);
    }
}

/// **A snapshot taken with the decoder mid-bit restores to decode the same
/// frames.**
///
/// The snapshot point is inside the first frame — a decoder with a partial
/// byte, a bit count and the cycle its current pulse started — and the frames
/// decoded after a restore have to equal the frames decoded without one.
#[test]
#[ignore = "needs the fw-esp32v3 test_rmt ELF; run through `just test-emu-esp32v3-boot`"]
fn a_snapshot_taken_inside_a_frame_restores_the_decoder_mid_bit() {
    let Some(mut m) = machine("rmt_chase", FrameSink::Memory) else {
        return;
    };
    // Inside the first frame ([`MID_FRAME_US`]); the assertion below is what
    // proves it rather than the arithmetic.
    m.run_until(&StopCondition::after_micros(MID_FRAME_US));
    assert!(
        m.pin_state()
            .decoders
            .get(&PAD)
            .is_some_and(|d| d.is_mid_frame()),
        "the snapshot point must be inside a frame"
    );
    assert!(m.frames(PAD).is_empty(), "no frame has closed yet");
    let snapshot = m.snapshot();

    m.run_until(&StopCondition::after_micros(PREFIX_US));
    let expected: Vec<Frame> = m.frames(PAD).to_vec();
    assert!(!expected.is_empty(), "frames closed after the snapshot");

    m.restore(&snapshot);
    m.run_until(&StopCondition::after_micros(PREFIX_US));
    assert_eq!(
        m.frames(PAD).to_vec(),
        expected,
        "the restored run decoded different frames"
    );
    println!(
        "rmt_chase: snapshot at {MID_FRAME_US} us mid-bit, {} frame(s) decoded identically \
         after restore",
        expected.len()
    );
}

// ---------------------------------------------------------------------------
// 4. The transcription itself
// ---------------------------------------------------------------------------

/// The published FNV-1a 32-bit vectors. If this fails, the copy in this file
/// disagrees with the five others — and the gate above would then be comparing
/// the guest against a different function while looking healthy.
#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a(b""), 0x811c_9dc5);
    assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
}

/// The record parser, against the line shape `fw-checks` renders.
#[test]
fn the_record_parser_reads_the_line_the_payload_prints() {
    let capture = "INFO - [rmt-chase] 256 LEDs on gpio18\n\
         [fw-check-json] {\"kind\":\"rmt-frame\",\"n\":0,\"leds\":256,\"lit\":1,\
         \"crc\":\"0xf88210a7\"}\n\
         [fw-check-json] {\"kind\":\"rmt-frame\",\"n\":1,\"leds\":256,\"lit\":1,\
         \"crc\":\"0xc234d93f\"}\n\
         [fw-check-json] {\"kind\":\"other\",\"n\":9}\n";
    assert_eq!(
        records(capture),
        vec![
            Record {
                n: 0,
                leds: 256,
                lit: 1,
                crc: 0xf882_10a7
            },
            Record {
                n: 1,
                leds: 256,
                lit: 1,
                crc: 0xc234_d93f
            },
        ]
    );
}

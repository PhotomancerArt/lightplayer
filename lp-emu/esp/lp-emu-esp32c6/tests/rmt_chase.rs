//! M5 P1 G1-1 and G1-3: the `test_rmt` harness (`esp32c6,server,test_rmt`)
//! transmits its 256-LED white chase through the RMT model word-exact, and
//! two runs are identical.
//!
//! The harness is `LedChannel` on RMT slot 0 with the one-channel plan (all
//! four blocks, a 192-word window, 96-word halves) driving GPIO18; every
//! frame is 256 × 24 bits, one word per bit, then the 300 µs latch and the
//! STOP the driver fills the rest of the half with. Its logger is the
//! blocking-with-timeout `Esp32UsbSerialIo`, which pays one 250 ms
//! `DRAIN_TIMEOUT` on the host-absent USB link before latching — so the
//! first frame starts at ≈ 359 ms of guest time, and the run is 800 ms
//! rather than the brief's 400 to hold the ≥ 20 frames it asks for.
//!
//! `#[ignore]`d for the usual reason (`test_support`); `just test-emu-c6`
//! runs it.

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::unpermute;
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, FrameSink, Outcome, StopCondition, TimeGrade,
    frame_record,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::periph::rmt::Pulse;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};
use lp_ws281x::{ColorOrder, PulseItem, STOP_WORD};
use sha2::{Digest, Sha256};

const GATE_US: u64 = 800_000;
const LEDS: usize = 256;
const DATA_WORDS: usize = LEDS * 24;
/// One data word: 100 ticks at 80 MHz / div_cnt 1 = 200 cycles.
const WORD_CYCLES: Cycles = 200;
/// The latch: 2 × 12,000 ticks = 48,000 cycles.
const LATCH_CYCLES: Cycles = 48_000;
/// `ceil(6144 / 96)` threshold events per frame.
const THR_PER_FRAME: usize = 64;

struct Run {
    m: Esp32C6Machine,
    outcome: Outcome,
    notes: Vec<String>,
}

fn run(grade: TimeGrade) -> Option<Run> {
    let elf = match fw_esp32c6_image(&FwImage::TEST_RMT) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("rmt_chase", &reason);
            return None;
        }
    };
    println!("rmt_chase: {}", elf.display());
    let buf = SharedBuffer::new();
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(grade)
        // P1's word and pulse logs are this gate's second decoder; a
        // machine leaves them off (`Rmt::keep_logs`).
        .rmt_logs(true)
        // A filter that matches no block: only the notes come through.
        .trace(Box::new(buf.clone()), vec!["NOTHING".to_string()])
        .build()
        .expect("the test_rmt image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    Some(Run {
        m,
        outcome,
        notes: buf.lines(),
    })
}

/// A complete frame: the words between one STOP and the next, STOP included.
struct Frame<'a> {
    words: &'a [(Cycles, u32)],
}

fn frames(words: &[(Cycles, u32)]) -> Vec<Frame<'_>> {
    let mut out = Vec::new();
    let mut from = 0;
    for (i, (_, w)) in words.iter().enumerate() {
        if *w == STOP_WORD {
            out.push(Frame {
                words: &words[from..=i],
            });
            from = i + 1;
        }
    }
    out
}

/// Decode a frame's data words into 24-bit-per-pixel RGB-order bytes as the
/// driver wrote them (GRB on the wire — irrelevant for white).
fn decode_pixels(frame: &Frame<'_>) -> Vec<[u8; 3]> {
    let data = &frame.words[..DATA_WORDS];
    let mut bits = Vec::with_capacity(DATA_WORDS);
    for (i, (_, w)) in data.iter().enumerate() {
        let item = PulseItem::decode(*w).unwrap_or_else(|| panic!("word {i} is STOP"));
        let (h, l) = (item.first, item.second);
        assert!(h.level && !l.level, "word {i}: {item:?}");
        let bit = match (h.ticks, l.ticks) {
            (32, 68) => false,
            (64, 36) => true,
            other => panic!("word {i}: not a WS2812 code: {other:?}"),
        };
        bits.push(bit);
    }
    bits.chunks_exact(24)
        .map(|px| {
            let mut out = [0u8; 3];
            for (b, byte) in out.iter_mut().enumerate() {
                for k in 0..8 {
                    if px[b * 8 + k] {
                        *byte |= 0x80 >> k;
                    }
                }
            }
            out
        })
        .collect()
}

#[test]
#[ignore = "needs the fw-esp32c6 test_rmt ELF; run through `just test-emu-c6`"]
fn the_test_rmt_chase_is_transmitted_word_exact() {
    let Some(Run { m, outcome, notes }) = run(TimeGrade::T1) else {
        return;
    };
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "expected the emulated deadline, got {outcome:?}"
    );
    assert_eq!(
        m.bus.unmapped_reads() + m.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    let spins: Vec<&String> = notes.iter().filter(|l| l.contains(" SPIN ")).collect();
    assert!(
        spins.iter().all(|l| l.contains("USB_DEVICE")),
        "a SPIN other than the USB one: {spins:?}"
    );

    let ended = m.rmt_frames_ended(0);
    assert!(ended >= 20, "{ended} frames ended in {GATE_US} us");
    let words = m.rmt_words(0);
    let frames = frames(words);
    assert_eq!(frames.len(), ended, "one STOP per tx_end");
    println!(
        "rmt_chase: {ended} frames, {} words, {} pulses, first frame at cycle {}",
        words.len(),
        m.rmt_pulses(0).len(),
        words[0].0
    );

    for (k, frame) in frames.iter().enumerate() {
        assert_eq!(
            frame.words.len(),
            DATA_WORDS + 2,
            "frame {k}: 6,144 data words + latch + STOP"
        );
        let pixels = decode_pixels(frame);
        assert_eq!(pixels.len(), LEDS);
        for (i, px) in pixels.iter().enumerate() {
            let expected = if i == k % LEDS {
                [10, 10, 10]
            } else {
                [0, 0, 0]
            };
            assert_eq!(*px, expected, "frame {k}, pixel {i}");
        }
        // The latch, then the STOP.
        let latch = PulseItem::decode(frame.words[DATA_WORDS].1).expect("the latch");
        assert!(!latch.first.level && !latch.second.level);
        assert_eq!((latch.first.ticks, latch.second.ticks), (12_000, 12_000));
        assert_eq!(frame.words[DATA_WORDS + 1].1, STOP_WORD);
        // Consecutive data words 200 cycles apart, the latch 48,000.
        for pair in frame.words[..=DATA_WORDS].windows(2) {
            assert_eq!(pair[1].0 - pair[0].0, WORD_CYCLES, "frame {k}");
        }
        assert_eq!(
            frame.words[DATA_WORDS + 1].0 - frame.words[DATA_WORDS].0,
            LATCH_CYCLES,
            "frame {k}: the latch word"
        );
    }

    // 64 `thr` notes per frame, between its `start` and its `end`.
    let mut thr_per_frame = Vec::new();
    let mut in_frame = false;
    let mut thr = 0usize;
    for line in &notes {
        if line.contains("RMT ch0 start ") {
            in_frame = true;
            thr = 0;
        } else if line.contains("RMT ch0 thr ") && in_frame {
            thr += 1;
        } else if line.contains("RMT ch0 end ") && in_frame {
            thr_per_frame.push(thr);
            in_frame = false;
        }
    }
    assert_eq!(thr_per_frame.len(), ended);
    assert!(
        thr_per_frame.iter().all(|&t| t == THR_PER_FRAME),
        "{thr_per_frame:?}"
    );
    assert!(notes.iter().any(|l| l.contains(
        "RMT ch0 start f_rmt=80000000 div_cnt=1 window=0..192 raddr=0 wrap=1 tx_lim=96"
    )));
    assert!(notes.iter().all(|l| !l.contains("RMT ch0 err")));
    assert!(notes.iter().all(|l| !l.contains("log cap")));
    // Nothing on UART0: the harness logs over USB-Serial-JTAG.
    assert!(m.uart0().is_empty());
}

#[test]
#[ignore = "needs the fw-esp32c6 test_rmt ELF; run through `just test-emu-c6`"]
fn two_runs_are_identical_and_t2_transmits_the_same_words() {
    let (Some(a), Some(b)) = (run(TimeGrade::T1), run(TimeGrade::T1)) else {
        return;
    };
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    let (pa, pb): (&[Pulse], &[Pulse]) = (a.m.rmt_pulses(0), b.m.rmt_pulses(0));
    assert!(!pa.is_empty());
    assert_eq!(pa, pb, "the pulse logs diverged");
    assert_eq!(a.m.rmt_words(0), b.m.rmt_words(0));
    assert_eq!(a.notes, b.notes);

    let Some(t2) = run(TimeGrade::T2) else {
        return;
    };
    assert!(
        matches!(t2.outcome, Outcome::Deadline { .. }),
        "{:?}",
        t2.outcome
    );
    assert_eq!(t2.m.bus.unmapped_reads() + t2.m.bus.unmapped_writes(), 0);
    let wa: Vec<u32> = a.m.rmt_words(0).iter().map(|(_, w)| *w).collect();
    let w2: Vec<u32> = t2.m.rmt_words(0).iter().map(|(_, w)| *w).collect();
    let n = wa.len().min(w2.len());
    assert!(n > 20 * (DATA_WORDS + 2));
    assert_eq!(wa[..n], w2[..n], "the word sequence differs between grades");
    let fa: Vec<usize> = frames(a.m.rmt_words(0))
        .iter()
        .map(|f| f.words.len())
        .collect();
    let f2: Vec<usize> = frames(t2.m.rmt_words(0))
        .iter()
        .map(|f| f.words.len())
        .collect();
    let n = fa.len().min(f2.len());
    assert!(n >= 20, "{n} frames under both grades");
    assert_eq!(
        fa[..n],
        f2[..n],
        "per-frame word counts differ between grades"
    );
    println!(
        "rmt_chase: t1 {} frames at {} cycles, t2 {} frames at {} cycles",
        fa.len(),
        a.m.cycles(),
        f2.len(),
        t2.m.cycles()
    );
}

// ---- M5 P2: the pad ------------------------------------------------------

/// The routing note the GPIO view writes when esp-hal's `with_pin` connects
/// the channel (M5 discovery §8: `func_out_sel_cfg[18].out_sel = 71`).
const ROUTE_NOTE: &str = "PIN gpio18 <- RMT_SIG_0 (out_sel=71";

/// The wire bytes of one frame, from P1's word log: the second decoder the
/// gate compares against, MSB first, three bytes per pixel.
fn wire_from_words(frame: &Frame<'_>) -> Vec<u8> {
    decode_pixels(frame).concat()
}

#[test]
#[ignore = "needs the fw-esp32c6 test_rmt ELF; run through `just test-emu-c6`"]
fn the_pad_carries_the_frames_the_words_describe() {
    let Some(Run { mut m, outcome, notes }) = run(TimeGrade::T1) else {
        return;
    };
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "expected the emulated deadline, got {outcome:?}"
    );

    // 1. The routing: gpio18, once, and no other pad.
    let routes: Vec<&String> = notes.iter().filter(|l| l.contains("PIN gpio")).collect();
    assert_eq!(
        routes.len(),
        1,
        "exactly one routing note, and it is gpio18's: {routes:?}"
    );
    assert!(routes[0].contains(ROUTE_NOTE), "{}", routes[0]);
    println!("rmt_chase: {}", routes[0]);
    let routed = m.routed_pads();
    assert_eq!(routed.len(), 1, "{routed:?}");
    assert_eq!(routed[0].0, PadId(18));
    assert_eq!(
        routed[0].1,
        RouteSource::Signal(SignalId(71), false),
        "RMT_SIG_0, not inverted"
    );

    // 2. The frames the pad carried, against the frames the words describe.
    let ended = m.rmt_frames_ended(0);
    let words = m.rmt_words(0).to_vec();
    let word_frames = frames(&words);
    let decoded = m.frames(18).to_vec();
    assert!(
        decoded.len() == ended || decoded.len() + 1 == ended,
        "{} decoded frames against {ended} tx_ends (the last is still open until its \
         reset arrives)",
        decoded.len()
    );
    assert!(decoded.len() >= 20, "{} frames on the pad", decoded.len());
    println!(
        "rmt_chase: {} frames on gpio18, {} edges, {ended} tx_ends",
        decoded.len(),
        m.pin_edges(18)
    );

    for (k, frame) in decoded.iter().enumerate() {
        assert_eq!(frame.n, k as u64);
        assert_eq!(frame.pad, PadId(18));
        assert_eq!(frame.error_count, 0, "frame {k}: {:?}", frame.errors);
        assert_eq!(frame.trailing_bits, 0, "frame {k}");
        assert_eq!(frame.bits, DATA_WORDS, "frame {k}: 6,144 bits");
        assert_eq!(frame.leds(), LEDS, "frame {k}");
        assert!(frame.is_complete(), "frame {k}");
        // The two decoders agree, byte for byte.
        assert_eq!(
            frame.wire,
            wire_from_words(&word_frames[k]),
            "frame {k}: the pad and the word log disagree"
        );
        // And the chase is where it should be: pixel k lit, the rest black.
        let rgb = unpermute(&frame.wire, ColorOrder::Grb);
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            let expected = if i == k % LEDS { [10, 10, 10] } else { [0, 0, 0] };
            assert_eq!(px, expected, "frame {k}, pixel {i}");
        }
    }

    // 3. The reset between frames: the 300 us latch plus the harness's 10 ms
    //    sleep (M5 P1: a frame is 1,276,800 cycles, then the sleep).
    let resets: Vec<f64> = decoded
        .iter()
        .filter_map(|f| f.reset_cycles)
        .map(|c| c as f64 / memmap::CYCLES_PER_US as f64)
        .collect();
    let reset_min = resets.iter().cloned().fold(f64::MAX, f64::min);
    let reset_max = resets.iter().cloned().fold(0.0, f64::max);
    println!("rmt_chase: reset {reset_min:.1}..{reset_max:.1} us between frames");
    assert!(
        (10_000.0..11_000.0).contains(&reset_min) && (10_000.0..11_000.0).contains(&reset_max),
        "the reset gap is the 300 us latch + the 10 ms sleep: {reset_min}..{reset_max} us"
    );

    // 4. The frame period: 1,276,800 cycles of transmission (7.98 ms) plus
    //    that sleep, so ~18 ms — a band from the geometry, reported.
    let periods: Vec<f64> = decoded
        .windows(2)
        .map(|w| (w[1].start - w[0].start) as f64 / memmap::CYCLES_PER_US as f64)
        .collect();
    let p_min = periods.iter().cloned().fold(f64::MAX, f64::min);
    let p_max = periods.iter().cloned().fold(0.0, f64::max);
    println!("rmt_chase: frame period {p_min:.1}..{p_max:.1} us");
    assert!(
        (17_000.0..19_000.0).contains(&p_min) && (17_000.0..19_000.0).contains(&p_max),
        "frame period {p_min}..{p_max} us"
    );

    // 5. The record the CLI would have written for the first frame.
    let record = frame_record(&decoded[0], m.strip(), Some(&routed[0].1));
    println!("rmt_chase: {}", &record[..record.len().min(240)]);
    assert!(record.contains("\"kind\":\"ws281x-frame\",\"pad\":18,\"signal\":\"RMT_SIG_0\""));
    assert!(record.contains("\"bits\":6144,\"leds\":256"));
    assert!(record.contains("\"errors\":0"));
    assert!(record.contains("\"complete\":true"));
    for line in m.pin_summaries() {
        println!("rmt_chase: {line}");
    }

    // 6. Nothing was lost on the way. The run's deadline falls inside the
    //    next frame, so the flush reports one more, incomplete.
    assert_eq!(m.bus.pins.dropped_edges(), 0);
    m.flush_frames();
    let after = m.frames(18).to_vec();
    assert_eq!(after.len(), decoded.len() + 1, "the open frame is flushed");
    assert!(!after[after.len() - 1].is_complete(), "it never latched");
    assert_eq!(after[after.len() - 1].reset_cycles, None);
    // Two edges per bit — a rise and a fall — and nothing else: the latch's
    // low follows the last bit's low, and the idle level at `tx_end` is the
    // level the pad already rests at. The run may have stopped between a
    // bit's rise and its fall, which is the one spare edge.
    let bits: usize = after.iter().map(|f| f.bits).sum();
    let edges = m.pin_edges(18) as usize;
    println!(
        "rmt_chase: {edges} edges for {bits} bits over {} frames ({} still open at the deadline)",
        after.len(),
        after[after.len() - 1].bits
    );
    assert!(
        edges == 2 * bits || edges == 2 * bits + 1,
        "{edges} edges for {bits} bits"
    );
}

#[test]
#[ignore = "needs the fw-esp32c6 test_rmt ELF; run through `just test-emu-c6`"]
fn two_runs_dump_identical_frames_and_a_snapshot_carries_a_decoder_mid_frame() {
    let dir = std::env::temp_dir();
    let paths = [
        dir.join("lp-emu-m5-p2-frames-a.jsonl"),
        dir.join("lp-emu-m5-p2-frames-b.jsonl"),
    ];
    let mut digests = Vec::new();
    for path in &paths {
        let elf = match fw_esp32c6_image(&FwImage::TEST_RMT) {
            Ok(p) => p,
            Err(reason) => {
                skip_notice("rmt_chase", &reason);
                return;
            }
        };
        let mut m = Esp32C6Builder::new()
            .app(AppSource::Path(elf))
            .strict(true)
            .time_grade(TimeGrade::T1)
            .dump_frames(FrameSink::File(path.clone()))
            .build()
            .expect("the test_rmt image builds a machine");
        let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
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
    assert_eq!(digests[0].0, digests[1].0, "two runs dumped different frames");
    assert_eq!(digests[0].1, digests[1].1);
    let first = String::from_utf8(digests[0].1.clone()).expect("utf-8");
    println!("rmt_chase: first record {}", first.lines().next().unwrap());
    for path in &paths {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
#[ignore = "needs the fw-esp32c6 test_rmt ELF; run through `just test-emu-c6`"]
fn a_snapshot_taken_inside_a_frame_restores_the_decoder_mid_bit() {
    let elf = match fw_esp32c6_image(&FwImage::TEST_RMT) {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("rmt_chase", &reason);
            return;
        }
    };
    let buf = SharedBuffer::new();
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(TimeGrade::T1)
        .trace(Box::new(buf.clone()), vec!["NOTHING".to_string()])
        .build()
        .expect("the test_rmt image builds a machine");

    // Inside the first frame: it starts at ~359 ms and runs 7.98 ms.
    m.run_until(&StopCondition::after_micros(363_000));
    assert!(
        m.pin_state()
            .decoders
            .get(&18)
            .is_some_and(|d| d.is_mid_frame()),
        "the snapshot point must be inside a frame"
    );
    assert!(m.frames(18).is_empty(), "no frame has closed yet");
    let snapshot = m.snapshot();
    let mark = buf.lines().len();

    m.run_until(&StopCondition::after_micros(400_000));
    let expected: Vec<lp_emu_esp_common::strip::ws281x::Frame> = m.frames(18).to_vec();
    let expected_notes: Vec<String> = buf.lines()[mark..].to_vec();
    assert!(!expected.is_empty(), "a frame closed after the snapshot");

    m.restore(&snapshot);
    let mark = buf.lines().len();
    m.run_until(&StopCondition::after_micros(400_000));
    assert_eq!(
        m.frames(18).to_vec(),
        expected,
        "the restored run decoded different frames"
    );
    assert_eq!(buf.lines()[mark..].to_vec(), expected_notes);
    println!(
        "rmt_chase: snapshot at 363 ms, {} frame(s) decoded identically after restore",
        expected.len()
    );
}

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
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade};
use lp_emu_esp32c6::periph::rmt::Pulse;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};
use lp_ws281x::{PulseItem, STOP_WORD};

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
            let expected = if i == k % LEDS { [10, 10, 10] } else { [0, 0, 0] };
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
    assert!(
        notes
            .iter()
            .any(|l| l.contains("RMT ch0 start f_rmt=80000000 div_cnt=1 window=0..192 raddr=0 wrap=1 tx_lim=96"))
    );
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
    assert!(matches!(t2.outcome, Outcome::Deadline { .. }), "{:?}", t2.outcome);
    assert_eq!(t2.m.bus.unmapped_reads() + t2.m.bus.unmapped_writes(), 0);
    let wa: Vec<u32> = a.m.rmt_words(0).iter().map(|(_, w)| *w).collect();
    let w2: Vec<u32> = t2.m.rmt_words(0).iter().map(|(_, w)| *w).collect();
    let n = wa.len().min(w2.len());
    assert!(n > 20 * (DATA_WORDS + 2));
    assert_eq!(wa[..n], w2[..n], "the word sequence differs between grades");
    let fa: Vec<usize> = frames(a.m.rmt_words(0)).iter().map(|f| f.words.len()).collect();
    let f2: Vec<usize> = frames(t2.m.rmt_words(0)).iter().map(|f| f.words.len()).collect();
    let n = fa.len().min(f2.len());
    assert!(n >= 20, "{n} frames under both grades");
    assert_eq!(fa[..n], f2[..n], "per-frame word counts differ between grades");
    println!(
        "rmt_chase: t1 {} frames at {} cycles, t2 {} frames at {} cycles",
        fa.len(),
        a.m.cycles(),
        f2.len(),
        t2.m.cycles()
    );
}

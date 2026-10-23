//! LCD_CAM + GDMA on the S3 (M9, the parallel WS281x output): esp-hal's
//! i8080 driver, fed by a GDMA descriptor chain out of internal RAM, drives
//! five uneven WS281x lanes, and the per-pad decoders the machine already runs
//! read every lane back **byte-exact**.
//!
//! The image is `fw-esp32s3 --features test_lcd_lanes`
//! (`lp-fw/fw-esp32s3/src/tests/test_lcd_lanes.rs`): lanes 0–4 on GPIO1–5
//! carrying 12, 9, 7, 5 and 3 LEDs, each byte a fixed function of its lane and
//! index ([`lane_byte`], the image's own), sent twice.
//!
//! Every figure is `lp-emu:esp32s3:t1`'s. The bytes transfer; the timing is
//! modelled and **ungraded** until a silicon capture grades it, so the time
//! checks here are the model's own arithmetic (a 400 ns slot is 96 CPU
//! cycles), never a claim about silicon.
//!
//! `#[ignore]`d for the usual reason (`lp_emu_esp32s3::test_support`):
//! `just test-emu-esp32s3-lcd` builds the image and runs this.

use lp_emu_esp32s3::machine::{AppSource, BootMode, Esp32S3Builder, StopCondition, UsbHost};
use lp_emu_esp32s3::memmap;
use lp_emu_esp32s3::test_support::{lcd_lanes_image, skip_notice};

/// The image's lanes: LEDs per lane, on GPIO1..=5.
const LANES: [usize; 5] = [12, 9, 7, 5, 3];
const DONE: &str = "[LCD-LANES] done";
/// One 400 ns slot at 240 MHz.
const SLOT_CYCLES: u64 = 96;

/// The image's `lane_byte`, the same arithmetic.
const fn lane_byte(lane: usize, k: usize) -> u8 {
    ((lane as u8 + 1).wrapping_mul(37)) ^ (k as u8).wrapping_mul(11) ^ 0x5a
}

#[test]
#[ignore = "needs the test_lcd_lanes image; `just test-emu-esp32s3-lcd`"]
fn five_uneven_lanes_decode_byte_exact_off_their_pads() {
    let elf = match lcd_lanes_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "five_uneven_lanes_decode_byte_exact_off_their_pads",
                &reason,
            );
            return;
        }
    };
    let mut m = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .boot_mode(BootMode::Direct)
        .usb_host(UsbHost::Attached { draining: true })
        .strict(true)
        .build()
        .expect("the harness image builds a machine");
    let stop = StopCondition {
        exit_on: Some(DONE.to_string()),
        ..StopCondition::after_micros(1_000_000)
    };
    m.run_until(&stop);
    // The last frame's latch: run on past it so its decoder closes it.
    let until = m.micros() + 1_000;
    m.run_until(&StopCondition::after_micros(until));
    m.flush_frames();
    let console = String::from_utf8_lossy(&m.usb_sj()).into_owned();
    assert!(console.contains(DONE), "{console}");
    assert!(
        !console.contains("result err"),
        "esp-hal saw no DMA error:\n{console}"
    );
    assert!(
        m.first_strict_violation().is_none(),
        "no unmodelled access: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(m.lcd_frames(), 2, "LCD_CAM shifted out both frames");

    let longest = LANES.iter().max().copied().unwrap() * 24;
    for (lane, &leds) in LANES.iter().enumerate() {
        let pad = lane as u8 + 1;
        let expected: Vec<u8> = (0..leds * 3).map(|k| lane_byte(lane, k)).collect();
        let frames = m.frames(pad);
        assert_eq!(frames.len(), 2, "gpio{pad}: two frames");
        for f in frames {
            assert!(
                f.is_complete(),
                "gpio{pad} frame {}: {} errors, {} trailing bits",
                f.n,
                f.error_count,
                f.trailing_bits
            );
            assert_eq!(f.wire, expected, "gpio{pad} frame {}", f.n);
        }
        // Every lane's frame starts on the same slot: lockstep.
        assert_eq!(frames[0].start, m.frames(1)[0].start, "gpio{pad} in step");
        // A lane's last falling edge is its last bit's high: slot 3·b + 1 or
        // + 2 of a frame whose bits are in 3-slot groups — so its span is the
        // model's own arithmetic, and the frame lasts as long as its longest
        // lane only through the latch.
        let span = frames[0].end - frames[0].start;
        let bits = (leds * 24) as u64;
        assert!(
            span > (bits - 1) * 3 * SLOT_CYCLES && span <= bits * 3 * SLOT_CYCLES,
            "gpio{pad}: span {span} cycles for {bits} bits"
        );
    }
    // The two frames are one bus frame apart: the longest lane's bits plus
    // the latch, plus the driver's own turnaround between `send`s.
    let one = m.frames(1);
    let gap = one[1].start - one[0].start;
    let bus = ((longest * 3) + 750) as u64 * SLOT_CYCLES;
    assert!(
        gap >= bus,
        "the second frame starts after the first's latch: {gap} < {bus}"
    );
    println!(
        "lp-emu:esp32s3:t1 — 5 lanes × 2 frames byte-exact; bus frame {} us, frame-to-frame {} us",
        bus / memmap::CYCLES_PER_US,
        gap / memmap::CYCLES_PER_US
    );
}

#[test]
fn the_cam_registers_are_unmapped_and_the_lcd_ones_are_not() {
    let mut m = Esp32S3Builder::new()
        .strict(true)
        .build()
        .expect("a bare machine builds");
    let base = memmap::periph::LCD_CAM;
    assert!(m.peek_word(base).is_some(), "lcd_clock is mapped");
    assert!(m.peek_word(base + 0x14).is_some(), "lcd_user is mapped");
    for cam in [0x04, 0x08, 0x0c] {
        assert!(
            m.peek_word(base + cam).is_none(),
            "+{cam:#x} (the CAM) refuses"
        );
    }
    assert!(
        m.peek_word(memmap::periph::DMA + 0x80).is_some(),
        "ch0.out_link"
    );
}

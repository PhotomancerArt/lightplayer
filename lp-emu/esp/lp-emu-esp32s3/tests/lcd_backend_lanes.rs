//! The **product** LCD_CAM WS281x backend (`fw-esp32s3`'s
//! `output::lcd::Esp32S3LcdWs281xDriver`, feature `lcd-ws281x`; S3 radio
//! roadmap M9) on the emulated S3: five uneven lanes opened through the
//! registry and the board profile's endpoints, sent with
//! `Ws281xOutput::start`/`wait_complete` and the backend's own encoder
//! (`lp_ws281x::parallel`, GRB on the wire), read back **byte-exact** by the
//! per-pad decoders.
//!
//! Where `lcd_cam_lanes.rs` proves the LCD_CAM and GDMA models against
//! esp-hal's driver with a plain encoder, this proves the backend the product
//! image ships behind `lcd-ws281x`, on the same models.
//!
//! The image is `fw-esp32s3 --features test_lcd_backend`
//! (`lp-fw/fw-esp32s3/src/tests/test_lcd_backend.rs`): D0–D4 (GPIO1–5)
//! carrying 12, 9, 7, 5 and 3 LEDs, wire byte `k` of lane `l` being
//! [`lane_byte`] (the image's own arithmetic) for frames 0 and 1. The image
//! overwrites its bytes with the next frame's as soon as `wait_complete`
//! returns, while the wire may still be sending: each frame decoding as its
//! own bytes is the proof that the backend's early release (render overlapping
//! the wire) never reads the caller's bytes after it. Figures are
//! `lp-emu:esp32s3:t1`'s: the bytes transfer, the timing is ungraded.
//!
//! `#[ignore]`d for the usual reason (`lp_emu_esp32s3::test_support`):
//! `just test-emu-esp32s3-lcd` builds the image and runs this.

use lp_emu_esp32s3::machine::{AppSource, BootMode, Esp32S3Builder, StopCondition, UsbHost};
use lp_emu_esp32s3::test_support::{lcd_backend_image, skip_notice};

/// LEDs per lane, on GPIO1..=5.
const LANES: [usize; 5] = [12, 9, 7, 5, 3];
const DONE: &str = "[LCD-BACKEND] done";

/// The image's `lane_byte`, the same arithmetic.
const fn lane_byte(lane: usize, k: usize, frame: u32) -> u8 {
    ((lane as u8 + 1).wrapping_mul(29)) ^ (k as u8).wrapping_mul(13) ^ 0xa5 ^ (frame as u8 * 0x3c)
}

#[test]
#[ignore = "needs the test_lcd_backend image; `just test-emu-esp32s3-lcd`"]
fn the_product_backend_sends_five_uneven_lanes_byte_exact() {
    let elf = match lcd_backend_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "the_product_backend_sends_five_uneven_lanes_byte_exact",
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
        ..StopCondition::after_micros(2_000_000)
    };
    m.run_until(&stop);
    // Run on past the last frame's latch so its decoders close it.
    let until = m.micros() + 1_000;
    m.run_until(&StopCondition::after_micros(until));
    m.flush_frames();
    let console = String::from_utf8_lossy(&m.usb_sj()).into_owned();
    assert!(console.contains(DONE), "{console}");
    assert!(!console.contains("FAIL"), "{console}");
    assert!(
        console.contains("[LCD-BACKEND] frame 0 ok")
            && console.contains("[LCD-BACKEND] frame 1 ok"),
        "both frames reported ok:\n{console}"
    );
    assert!(
        m.first_strict_violation().is_none(),
        "no unmodelled access: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(m.lcd_frames(), 2, "one bus frame per start round");

    for (lane, &leds) in LANES.iter().enumerate() {
        let pad = lane as u8 + 1;
        let frames = m.frames(pad);
        assert_eq!(frames.len(), 2, "gpio{pad}: two frames");
        for (n, f) in frames.iter().enumerate() {
            // Frame n's own bytes, though the image overwrote them with frame
            // n + 1's the moment `wait_complete` returned (the overlap).
            let expected: Vec<u8> = (0..leds * 3)
                .map(|k| lane_byte(lane, k, n as u32))
                .collect();
            assert!(
                f.is_complete(),
                "gpio{pad} frame {}: {} errors, {} trailing bits",
                f.n,
                f.error_count,
                f.trailing_bits
            );
            assert_eq!(f.wire, expected, "gpio{pad} frame {}", f.n);
        }
        assert_eq!(frames[0].start, m.frames(1)[0].start, "gpio{pad} in step");
    }
    println!("lp-emu:esp32s3:t1 — the product LCD backend: 5 lanes × 2 frames byte-exact");
}

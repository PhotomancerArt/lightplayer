//! Serial readout of the frames the RMT channels transmit.
//!
//! An LED strip is not a measuring instrument: it shows that *something*
//! rendered, never *which bytes*. So this module decorates the RMT output's
//! write path with the same transcript the M3/M4 `SerialReadoutWs281xDriver`
//! printed before real output existed — and for the same reason. The M4
//! hardware walk (`scripts/m4-hardware-walk.sh`) diffs these lines against a
//! host render byte for byte, which is how "the S3's Xtensa JIT renders
//! correctly" is a claim with a number attached rather than a photograph.
//!
//! ## Why it is a cargo feature and not a runtime flag
//!
//! Hex-formatting every frame costs time on the render path and bandwidth on
//! the USB-Serial-JTAG link the transport also runs on. A runtime flag would
//! still compile the formatting in and still branch on it per frame. `cfg`
//! means an app build that did not ask for the readout contains none of this —
//! no formatter, no checksum, not even the counter — which is the only version
//! of "opt-in" worth the observability.
//!
//! ## Volume
//!
//! A full pixel dump every frame would drown the link (256 LEDs × 60 fps ≈
//! 46 KB/s of hex before framing), and that link is the same one the host is
//! talking to. So:
//!
//! - **One full hex dump** per output channel, on the first frame after open or
//!   after a resize, capped at [`MAX_DUMP_LEDS`]. That proves the pixel path
//!   end to end, exactly once, when the interesting thing just happened.
//! - **A summary line** thereafter, at most one per [`REPORT_EVERY_FRAMES`]
//!   frames: a checksum over the frame plus the lit-LED count and the first few
//!   pixels. The checksum is what distinguishes "rendering, and the picture is
//!   changing" from "rendering the same frame forever" from "not rendering" —
//!   the three states a walk actually needs to tell apart.
//!
//! The line shapes are load-bearing, not cosmetic: `scripts/m4-hardware-walk.sh`
//! greps for `[OUT] dump` and for the `rgb=` token, and
//! `lp-app/lpa-server/tests/shader_oracle_frame.rs` mirrors them on the host so
//! the two transcripts line up without either side being sliced by hand.
//!
//! ⚠️ There is a **third** copy: `lp-fw/fw-esp32v3/src/output/rmt/frame_dump.rs`
//! is a byte-for-byte port of this module, so the classic ESP32's M7 gate can
//! reuse the same walk script and the same host comparator with no per-chip
//! branch. Two firmwares under separate toolchains with no shared chip-side
//! library, so the duplication is deliberate — but a format string changed here
//! and not there silently breaks the classic's gate.

use lpc_hardware::HwEndpointId;

/// How many frames pass between summary lines. ~1 s at 60 fps.
const REPORT_EVERY_FRAMES: u32 = 60;

/// Cap on the one-shot full dump, in LEDs. 64 LEDs is 192 bytes ≈ 400 hex
/// characters — one long line, not a flood. `examples/shader-oracle` is sized
/// to exactly this so its dump is the *whole* frame.
pub const MAX_DUMP_LEDS: usize = 64;

/// How many leading pixels each summary line carries.
const SUMMARY_PIXELS: usize = 4;

/// Announce an opened output. Carries the frame size, which is what a later
/// `leds=` on a dump line has to agree with.
pub fn log_open(endpoint_id: &HwEndpointId, byte_count: u32) {
    log::info!(
        "[OUT] open endpoint={endpoint_id} bytes={byte_count} leds={} (frame-dump build)",
        byte_count / 3
    );
}

/// Per-output readout state: which frame we are on, and whether the next one is
/// owed a full dump.
pub struct FrameDump {
    frame: u32,
    /// Set whenever the next frame should be dumped in full: at open, and again
    /// after any resize.
    dump_next: bool,
    /// Armed alongside `dump_next`, and kept armed across all-black frames:
    /// the first frame after a project load is the compile-window black
    /// fallback (ADR 2026-08-03-memory-pressure-at-compile-safe-points), so
    /// the one-shot dump alone describes exactly the frame a hardware walk
    /// cannot use as evidence. When the first *lit* frame arrives this arms
    /// [`Self::lit_dump_countdown`] instead of dumping immediately — the
    /// instant the shader finishes compiling is also the moment the UART
    /// writer queue is flooded (the `compilation succeeded` burst, the
    /// PR #300 interleaving defect), and a dump printed into that flood is
    /// dropped end to end. A project that never lights dumps only once, at
    /// open — this never floods.
    dump_next_lit: bool,
    /// Frames remaining until the deferred lit dump fires; 0 = none pending.
    lit_dump_countdown: u32,
}

/// How long after the first lit frame the deferred full dump fires. ~0.6 s at
/// 50 fps: comfortably past the post-compile log burst, comfortably before
/// the first [`REPORT_EVERY_FRAMES`] summary.
const LIT_DUMP_DELAY_FRAMES: u32 = 30;

impl FrameDump {
    pub fn new() -> Self {
        Self {
            frame: 0,
            dump_next: true,
            dump_next_lit: true,
            lit_dump_countdown: 0,
        }
    }

    /// Report a frame that has just been transmitted. Called after the send
    /// completes, so the transcript describes bytes that actually reached the
    /// wire rather than bytes that were merely queued.
    pub fn on_write(&mut self, data: &[u8]) {
        self.frame = self.frame.wrapping_add(1);
        if self.dump_next_lit && lit_led_count(data) > 0 {
            self.dump_next_lit = false;
            self.lit_dump_countdown = LIT_DUMP_DELAY_FRAMES;
        }
        let lit_dump_due = self.lit_dump_countdown > 0 && {
            self.lit_dump_countdown -= 1;
            self.lit_dump_countdown == 0
        };
        if core::mem::take(&mut self.dump_next) || lit_dump_due {
            self.dump(data);
        } else if self.frame.is_multiple_of(REPORT_EVERY_FRAMES) {
            self.report(data);
        }
    }

    /// A resize changes what the next dump means, so it re-arms one.
    pub fn on_resize(&mut self, byte_count: u32) {
        log::info!("[OUT] resize bytes={byte_count} leds={}", byte_count / 3);
        self.dump_next = true;
        self.dump_next_lit = true;
        self.lit_dump_countdown = 0;
    }

    fn report(&self, data: &[u8]) {
        let leds = data.len() / 3;
        log::info!(
            "[OUT] frame={} leds={leds} crc={:#010x} lit={} first={}",
            self.frame,
            frame_checksum(data),
            lit_led_count(data),
            LeadingPixels(data),
        );
    }

    fn dump(&self, data: &[u8]) {
        let leds = data.len() / 3;
        let shown = leds.min(MAX_DUMP_LEDS);
        log::info!(
            "[OUT] dump frame={} leds={leds} shown={shown} crc={:#010x} rgb={}",
            self.frame,
            frame_checksum(data),
            HexPixels(&data[..shown * 3]),
        );
    }
}

/// FNV-1a over the frame bytes. Cheap, allocation-free, and sensitive enough
/// that a single changed channel changes the reported value — which is the
/// whole point of printing it.
fn frame_checksum(data: &[u8]) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// LEDs with any non-zero channel. Distinguishes "rendered black" from "did not
/// render", which a checksum alone cannot do at a glance.
fn lit_led_count(data: &[u8]) -> usize {
    data.chunks_exact(3)
        .filter(|led| led.iter().any(|c| *c != 0))
        .count()
}

/// `(r,g,b) (r,g,b) …` for the first [`SUMMARY_PIXELS`] LEDs. A `Display`
/// adapter rather than a `String` so the summary line allocates nothing.
struct LeadingPixels<'a>(&'a [u8]);

impl core::fmt::Display for LeadingPixels<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, led) in self.0.chunks_exact(3).take(SUMMARY_PIXELS).enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "({},{},{})", led[0], led[1], led[2])?;
        }
        Ok(())
    }
}

/// Contiguous lowercase hex for the one-shot dump.
struct HexPixels<'a>(&'a [u8]);

impl core::fmt::Display for HexPixels<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

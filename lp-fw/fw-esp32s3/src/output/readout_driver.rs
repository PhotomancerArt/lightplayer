//! A WS281x driver that reports frames over serial instead of driving LEDs.
//!
//! `LpServer` requires an output provider — it is not an `Option` — so a driver
//! exists on this board whether or not any LED hardware does. M3 does not port
//! the RMT/ws281x driver, so rather than discard the frames this driver
//! *reports* them. That is what makes the render path observable on a bare S3
//! with nothing wired to it, and it is why M4 can tell "the shader node ran"
//! from "the shader node was inert" without a single LED.
//!
//! ## Volume
//!
//! A full pixel dump every frame would drown the USB-Serial-JTAG link the
//! transport also runs on (256 LEDs × 60 fps ≈ 46 KB/s of hex before framing),
//! and the log path is the same link the host is talking to. So:
//!
//! - **One full hex dump** per output channel, on the first frame after open or
//!   after a resize, capped at [`MAX_DUMP_LEDS`]. That proves the pixel path
//!   end to end, exactly once, when the interesting thing just happened.
//! - **A summary line** thereafter, at most one per [`REPORT_EVERY_FRAMES`]
//!   frames: a checksum over the frame plus the lit-LED count and the first few
//!   pixels. The checksum is what distinguishes "rendering, and the picture is
//!   changing" from "rendering the same frame forever" from "not rendering" —
//!   the three states this milestone actually needs to tell apart.
//!
//! ## Why it delegates to `VirtualWs281xDriver`
//!
//! Endpoint enumeration, capability checks, and resource leases are registry
//! semantics, not chip semantics. Reimplementing them here would fork behaviour
//! the studio's endpoint list depends on. So this driver wraps the checked-in
//! virtual driver and decorates only the opened output. Its endpoints therefore
//! carry the `virtual-ws281x-rmt0` driver id, which is accurate: nothing here
//! reaches a pin.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;

use lpc_hardware::{
    HardwareEndpointError, HwDriver, HwEndpoint, HwEndpointId, HwRegistry, OutputError,
    VirtualWs281xDriver, Ws281xConfig, Ws281xDriver, Ws281xOutput,
};

/// RMT timing channel the endpoints claim. Matches the board manifest's
/// `/rmt/ws281x0` resource; nothing is programmed on it.
const RMT_CHANNEL: u8 = 0;

/// How many frames pass between summary lines. ~1 s at 60 fps.
const REPORT_EVERY_FRAMES: u32 = 60;

/// Cap on the one-shot full dump, in LEDs. 64 LEDs is 192 bytes ≈ 400 hex
/// characters — one long line, not a flood.
const MAX_DUMP_LEDS: usize = 64;

/// How many leading pixels each summary line carries.
const SUMMARY_PIXELS: usize = 4;

/// WS281x driver whose opened outputs report to serial rather than to hardware.
pub struct SerialReadoutWs281xDriver {
    inner: VirtualWs281xDriver,
}

impl SerialReadoutWs281xDriver {
    pub fn new(registry: Rc<HwRegistry>) -> Self {
        Self {
            inner: VirtualWs281xDriver::new(registry, RMT_CHANNEL),
        }
    }
}

impl HwDriver for SerialReadoutWs281xDriver {
    fn driver_id(&self) -> &str {
        self.inner.driver_id()
    }

    fn display_label(&self) -> &str {
        self.inner.display_label()
    }
}

impl Ws281xDriver for SerialReadoutWs281xDriver {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        self.inner.endpoints()
    }

    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        let byte_count = config.byte_count();
        let inner = self.inner.open(endpoint_id, config)?;
        log::info!(
            "[OUT] open endpoint={endpoint_id} bytes={byte_count} leds={} (serial readout; no LEDs driven)",
            byte_count / 3
        );
        Ok(Box::new(SerialReadoutWs281xOutput::new(inner)))
    }
}

/// Opened output: forwards every frame to the virtual output (which keeps the
/// registry lease alive and validates lengths) and reports it.
struct SerialReadoutWs281xOutput {
    inner: Box<dyn Ws281xOutput>,
    frame: u32,
    /// Set whenever the next frame should be dumped in full: at open, and again
    /// after any resize.
    dump_next: bool,
}

impl SerialReadoutWs281xOutput {
    fn new(inner: Box<dyn Ws281xOutput>) -> Self {
        Self {
            inner,
            frame: 0,
            dump_next: true,
        }
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

impl Ws281xOutput for SerialReadoutWs281xOutput {
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        self.inner.write(data)?;
        self.frame = self.frame.wrapping_add(1);
        if core::mem::take(&mut self.dump_next) {
            self.dump(data);
        } else if self.frame.is_multiple_of(REPORT_EVERY_FRAMES) {
            self.report(data);
        }
        Ok(())
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        let byte_count = config.byte_count();
        self.inner.resize(config)?;
        log::info!("[OUT] resize bytes={byte_count} leds={}", byte_count / 3);
        self.dump_next = true;
        Ok(())
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

//! The one [`lp_ws281x::Ws281xDriver`] instance on this chip, and the RMT
//! interrupt that feeds it.
//!
//! There is exactly one RMT peripheral, one RMT interrupt line, and therefore
//! one driver: the four TX channels are *channels of it*, not four drivers.
//! Every per-channel decision (timing, frame in flight, statistics) already
//! lives inside [`lp_ws281x::ChannelState`], so nothing here needs a second
//! layer of per-channel state — which is why this module holds a `static` and
//! the endpoint-facing driver holds none.
//!
//! `Ws281xDriver::new` is `const` and every field of `ChannelState` is
//! an atomic, so this needs neither `static mut` nor a `StaticCell`: the
//! handler and thread context share a `&'static`.

use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::rmt::Rmt;
use esp_hal::time::{Duration, Rate};
use lp_ws281x::Ws281xDriver;

use super::s3_rmt::{S3Rmt, TX_CHANNELS};

/// RMT source clock. Divider 1 makes one tick 12.5 ns, which is what
/// [`lp_ws281x::PulseCodes::DEFAULT_CLOCK_HZ`] assumes.
pub const RMT_CLOCK: Rate = Rate::from_mhz(80);

/// A frame that has not completed within this long has hung; abort it and
/// report rather than spinning forever. The longest frame the output provider
/// can ask for (256 LEDs) is ~7.7 ms on the wire.
pub const FRAME_TIMEOUT: Duration = Duration::from_millis(50);

/// The driver, shared between thread context and the interrupt handler.
pub static DRIVER: Ws281xDriver<S3Rmt, TX_CHANNELS> = Ws281xDriver::new(S3Rmt::new());

/// Set once the RMT interrupt handler has been bound.
///
/// fw-esp32c6 re-registers its handler on every channel construction and says
/// in a comment that it should not. Here it does not: with several endpoints
/// opening independently, rebinding is not a rare accident but the normal
/// case, and a handler swapped while a frame is in flight loses that frame's
/// refills.
static ISR_INSTALLED: AtomicBool = AtomicBool::new(false);

/// The RMT interrupt entry point: a trampoline and nothing else.
///
/// Placed in IRAM with `#[ram]` — a flash-cache miss here is exactly the
/// latency the guard word exists to survive, so it should not be
/// self-inflicted. No logging, no allocation: the core documents
/// [`Ws281xDriver::on_interrupt`] as the whole of the handler's work.
///
/// One entry services all four channels. With a memory block each they cross
/// their half boundaries within microseconds of one another, so coincident
/// causes are the rule rather than the exception — and dispatching them in one
/// pass is `on_interrupt`'s job, not this trampoline's.
#[esp_hal::ram]
extern "C" fn rmt_isr() {
    DRIVER.on_interrupt();
}

/// Bind [`rmt_isr`] at the highest priority esp-hal can dispatch, exactly once.
pub fn install_isr(rmt: &mut Rmt<'_, esp_hal::Blocking>) {
    if ISR_INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    rmt.set_interrupt_handler(InterruptHandler::new(rmt_isr, Priority::max()));
}

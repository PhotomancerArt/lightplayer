//! `log` and printf onto the link's log channel.
//!
//! Every `log::info!` (and every [`lab_printf!`]) formats into one fixed
//! [`LogRing`] under a critical section: no allocation, no I/O, safe from any
//! task. Each pipe's task moves records from the ring into its link's log
//! channel while that link is up ([`Link::pump_log`](lp_link::Link::pump_log)).
//! While no link is up the ring keeps the newest records and counts the ones
//! it dropped, and the next record out says how many.
//!
//! Boot text before the logger exists, and panics, go raw to the USB pipe
//! through esp-println, outside frames: the host sees them as
//! `LinkEvent::Text`.

use core::cell::RefCell;

use critical_section::Mutex;
use lp_link::log_ring::LogRing;

/// Bytes of log records the board holds while no link is up.
pub const LOG_RING_BYTES: usize = 4096;

pub static LOG_RING: Mutex<RefCell<LogRing<LOG_RING_BYTES>>> =
    Mutex::new(RefCell::new(LogRing::new()));

struct RingLogger;

static LOGGER: RingLogger = RingLogger;

impl log::Log for RingLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        critical_section::with(|cs| {
            lp_link::log_ring::push_log_record(&mut LOG_RING.borrow_ref_mut(cs), record);
        });
    }

    fn flush(&self) {}
}

/// Install the ring as the `log` backend.
pub fn init() {
    // SAFETY-free: `set_logger` fails only if a logger is already set, and
    // nothing else in this image sets one.
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}

/// Move records into `link`'s log channel (the caller's pipe task).
pub fn pump<A: lp_link::Arq>(link: &mut lp_link::Link<A>, now: lp_link::Micros) {
    critical_section::with(|cs| {
        link.pump_log(now, &mut LOG_RING.borrow_ref_mut(cs), lp_link::CH_LOG);
    });
}

/// printf onto the log channel, with no `log` crate in the way: the shape the
/// product's `printf`-style call sites would use.
#[macro_export]
macro_rules! lab_printf {
    ($($arg:tt)*) => {
        critical_section::with(|cs| {
            $crate::tests::comms_lab::lab_logger::LOG_RING
                .borrow_ref_mut(cs)
                .push_fmt(lp_link::log_ring::LEVEL_INFO, ::core::format_args!($($arg)*));
        })
    };
}

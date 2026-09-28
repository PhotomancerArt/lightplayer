//! `log` onto the host link's log channel (lp-link channel 2, D6 of plan
//! `lp-link-usb-cutover`).
//!
//! Every `log::info!` (and every JIT `__host_log`, which goes through the same
//! facade — see [`crate::jit_fns`]) formats into one fixed [`LogRing`] under a
//! critical section: no allocation, no I/O, safe from any task or thread. The
//! USB link task moves records from the ring into its link's log channel
//! while the link is up and its peer is not stalled ([`pump`]). While no host
//! is there the
//! ring keeps the newest records and counts the ones it dropped, and the next
//! record out says how many.
//!
//! One record is `level ‖ "<module path>: <message>"`, cut at
//! [`lp_link::log_ring::MAX_RECORD_TEXT`] bytes. The host renders it as the
//! console line this image used to write raw (`[INFO] module: message`).
//!
//! Boot text before the logger exists, and panics, still go raw to the USB
//! endpoint through esp-println, outside frames: the host sees them as
//! `LinkEvent::Text`. The comms lab's `lab_logger` is the template
//! (`fw-esp32c6/src/tests/comms_lab/lab_logger.rs`).

use core::cell::RefCell;
use core::fmt::Write;

use critical_section::Mutex;
use lp_link::log_ring::LogRing;

/// Bytes of log records the board holds while no host is draining them.
pub const LOG_RING_BYTES: usize = 4096;

/// The one ring every log call writes into.
pub static LOG_RING: Mutex<RefCell<LogRing<LOG_RING_BYTES>>> =
    Mutex::new(RefCell::new(LogRing::new()));

/// Default for the process-global `log::max_level()` gate applied at init,
/// the same as [`crate::logger`]'s: the client moves it at runtime with the
/// wire `SetLogLevel` command, and a reboot reverts to this.
const LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;

/// Install the ring as the `log` backend.
///
/// The logger itself is permissive: the global `log::max_level()` is the one
/// gate, exactly as [`crate::logger::init`] arranges it, so a raised level on
/// the wire is not masked by a cap here.
pub fn init() {
    // `set_logger` fails only if a logger is already set; nothing else in an
    // image that calls this sets one.
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LOG_LEVEL);
    }
}

/// Move up to `max` records into `link`'s log channel. Called by the link
/// task; does nothing while the link is down or stalled (the ring keeps the
/// records).
///
/// One record per critical section, not `Link::pump_log` under one: that
/// holds the ring's lock for the whole batch (up to the link's 32-record
/// datagram queue, each popped byte by byte and copied onto the heap), and
/// with interrupts masked that long the RMT refill misses its deadline. A
/// record the link refuses (its datagram queue full) is lost and counted by
/// the link (`datagrams_dropped`); `max` keeps that rare.
pub fn pump<A: lp_link::Arq>(link: &mut lp_link::Link<A>, now: lp_link::Micros, max: usize) {
    if link.state() != lp_link::LinkState::Established || link.is_stalled(now) {
        return;
    }
    let mut record = [0u8; 1 + lp_link::log_ring::MAX_RECORD_TEXT];
    for _ in 0..max {
        let Some(n) =
            critical_section::with(|cs| LOG_RING.borrow_ref_mut(cs).pop_into(&mut record))
        else {
            return;
        };
        if link.send(lp_link::CH_LOG, &record[..n]).is_err() {
            return;
        }
    }
}

/// Records the ring has dropped since boot (while no host drained it).
pub fn dropped_total() -> u32 {
    critical_section::with(|cs| LOG_RING.borrow_ref(cs).dropped_total())
}

struct RingLogger;

static LOGGER: RingLogger = RingLogger;

impl log::Log for RingLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        // esp-rtos's scheduler chatter would fill the ring (the old logger
        // dropped it too).
        let module_path = record.module_path().unwrap_or("unknown");
        if module_path.starts_with("esp_rtos") {
            return;
        }
        // Format OUTSIDE the critical section: a `Debug` of a big value runs
        // in full even though only the first `MAX_RECORD_TEXT` bytes are
        // kept, and interrupts (the RMT refill among them) must not wait on
        // it. Only the copy into the ring holds the lock.
        let mut line = RecordText::new();
        let _ = write!(line, "{}: {}", module_path, record.args());
        let level = lp_link::log_ring::level_of(record.level());
        critical_section::with(|cs| {
            LOG_RING.borrow_ref_mut(cs).push(level, line.as_bytes());
        });
    }

    fn flush(&self) {}
}

/// One record's text, cut at the ring's per-record limit.
struct RecordText {
    buf: [u8; lp_link::log_ring::MAX_RECORD_TEXT],
    len: usize,
}

impl RecordText {
    const fn new() -> Self {
        Self {
            buf: [0; lp_link::log_ring::MAX_RECORD_TEXT],
            len: 0,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl core::fmt::Write for RecordText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    /// A record is `level ‖ "module: message"`, cut at the ring's limit, and
    /// esp-rtos's chatter never reaches the ring. (The one test that touches
    /// the global ring.)
    #[test]
    fn a_record_carries_its_level_and_module_and_is_cut_at_the_limit() {
        let long = "x".repeat(500);
        for (module, text) in [
            ("esp_rtos::scheduler", "tick"),
            ("fw_esp32_common::server_loop", "[perf] frame=1"),
            ("m", long.as_str()),
        ] {
            LOGGER.log(
                &log::Record::builder()
                    .level(log::Level::Warn)
                    .module_path(Some(module))
                    .args(format_args!("{text}"))
                    .build(),
            );
        }
        let mut out = [0u8; 256];
        let pop = |out: &mut [u8; 256]| {
            critical_section::with(|cs| LOG_RING.borrow_ref_mut(cs).pop_into(out))
        };
        let n = pop(&mut out).unwrap();
        assert_eq!(out[0], lp_link::log_ring::LEVEL_WARN);
        assert_eq!(&out[1..n], b"fw_esp32_common::server_loop: [perf] frame=1");
        let n = pop(&mut out).unwrap();
        assert_eq!(n, 1 + lp_link::log_ring::MAX_RECORD_TEXT, "cut");
        assert!(out[1..n].starts_with(b"m: xxx"));
        assert!(pop(&mut out).is_none(), "esp_rtos was dropped");
    }
}

//! Dev-only: a tee of every raw byte the browser's device port reads, for a
//! hardware sitting that needs the bytes Studio saw (Studio's `?wire-capture=1`).
//!
//! Web Serial holds a port exclusively, so a second reader cannot capture
//! what a board sends while Studio talks to it. This tee sits where every
//! chunk the JS read pump hands to Rust passes (`browser_serial::take_reads`),
//! before any splitting, so the capture is exactly what arrived, in order —
//! `lp-cli wire unpack --sizes < capture` reads it like a serial log.
//!
//! Off unless enabled; capped at [`WIRE_CAPTURE_CAP`] bytes, after which it
//! drops the rest and says so once. Not a setting: nothing persists it.

use std::cell::RefCell;

/// The most a capture holds: 16 MiB.
pub const WIRE_CAPTURE_CAP: usize = 16 * 1024 * 1024;

thread_local! {
    static CAPTURE: RefCell<Option<WireCapture>> = const { RefCell::new(None) };
}

/// Start capturing (`true`) or stop and forget what was captured (`false`).
pub fn set_wire_capture(enabled: bool) {
    CAPTURE.with(|capture| {
        *capture.borrow_mut() = enabled.then(|| WireCapture::new(WIRE_CAPTURE_CAP));
    });
}

/// Tee `bytes` into the capture, when one is running. Cheap when not.
/// Returns `true` the one time the cap is reached, for the caller to say so.
pub fn capture_wire_bytes(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    CAPTURE.with(|capture| {
        capture
            .borrow_mut()
            .as_mut()
            .is_some_and(|capture| capture.push(bytes))
    })
}

/// A copy of everything captured so far (empty when no capture is running).
/// The capture keeps running.
pub fn wire_capture_bytes() -> Vec<u8> {
    CAPTURE.with(|capture| {
        capture
            .borrow()
            .as_ref()
            .map(|capture| capture.bytes.clone())
            .unwrap_or_default()
    })
}

/// A capped byte buffer that drops at the cap.
struct WireCapture {
    bytes: Vec<u8>,
    cap: usize,
    /// Whether it has already said it is full.
    full: bool,
}

impl WireCapture {
    fn new(cap: usize) -> Self {
        Self {
            bytes: Vec::new(),
            cap,
            full: false,
        }
    }

    /// Append what fits; `true` the first time something did not.
    fn push(&mut self, bytes: &[u8]) -> bool {
        let room = self.cap - self.bytes.len();
        let take = room.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..take]);
        if take < bytes.len() && !self.full {
            self.full = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_is_exactly_what_arrived_in_order() {
        set_wire_capture(true);
        assert!(!capture_wire_bytes(b"boot\n"));
        assert!(!capture_wire_bytes(&[0, 1, 2, b'\n', 0]));
        assert!(!capture_wire_bytes(b"M!{}\n"));
        assert_eq!(wire_capture_bytes(), b"boot\n\0\x01\x02\n\0M!{}\n");
        set_wire_capture(false);
        assert!(wire_capture_bytes().is_empty());
    }

    #[test]
    fn no_capture_keeps_nothing() {
        set_wire_capture(false);
        assert!(!capture_wire_bytes(b"ignored"));
        assert!(wire_capture_bytes().is_empty());
    }

    #[test]
    fn the_cap_drops_the_rest_and_says_so_once() {
        let mut capture = WireCapture::new(4);
        assert!(!capture.push(b"ab"));
        assert!(capture.push(b"cdef"));
        assert!(!capture.push(b"gh"));
        assert_eq!(capture.bytes, b"abcd");
    }
}

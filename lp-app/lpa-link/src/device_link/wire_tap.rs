//! A tap on every raw byte chunk a browser transport writes or reads, for
//! Studio's session recorder (`?record=<url>`).
//!
//! Each transport calls [`tap_wire`] at its one byte chokepoint — Web
//! Serial's `write_line` / `take_reads`, Bluetooth's `write` /
//! `take_bytes`, the tab emulator's `write` / `take_bytes` — with the
//! chunk exactly as it crossed, before any splitting. The web edge
//! installs a callback with [`set_wire_tap`] while it is recording and
//! turns each chunk into a sink line. The `?emu=ws://…`, `?emu=tab` and
//! `?ble=emu` polyfills sit behind those same calls, so they are covered.
//!
//! This is the hot path: with no tap installed, [`tap_wire`] is one
//! thread-local check and returns.
//!
//! [`wire_capture`](super::wire_capture) (`?wire-capture=1`) is the older,
//! narrower sibling: an in-memory buffer of what Web Serial reads, for
//! `lpWireCapture()`. It stays as it is.

use std::cell::RefCell;

/// Which way a chunk crossed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireTapDir {
    /// Studio wrote it to the device.
    Tx,
    /// Studio read it from the device.
    Rx,
}

impl WireTapDir {
    /// `tx` / `rx`, the recorder's rendering.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tx => "tx",
            Self::Rx => "rx",
        }
    }
}

/// One chunk as it crossed a transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireTapChunk<'a> {
    pub dir: WireTapDir,
    /// `serial`, `ble` or `emu-tab`.
    pub transport: &'static str,
    /// The transport's own port/session id.
    pub port: u32,
    pub bytes: &'a [u8],
}

/// The callback a recorder installs.
pub type WireTap = Box<dyn Fn(WireTapChunk<'_>)>;

thread_local! {
    static TAP: RefCell<Option<WireTap>> = const { RefCell::new(None) };
}

/// Install (`Some`) or remove (`None`) the tap.
pub fn set_wire_tap(tap: Option<WireTap>) {
    TAP.with(|slot| *slot.borrow_mut() = tap);
}

/// Hand one chunk to the tap, when one is installed. Empty chunks are not
/// chunks and are dropped.
pub fn tap_wire(dir: WireTapDir, transport: &'static str, port: u32, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    TAP.with(|slot| {
        // `try_borrow`: a tap that (indirectly) writes to a transport must
        // not panic the page; its own bytes are simply not re-tapped.
        let Ok(slot) = slot.try_borrow() else {
            return;
        };
        if let Some(tap) = slot.as_ref() {
            tap(WireTapChunk {
                dir,
                transport,
                port,
                bytes,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn with_no_tap_a_chunk_goes_nowhere() {
        set_wire_tap(None);
        tap_wire(WireTapDir::Tx, "serial", 1, b"M!{}\n");
    }

    #[test]
    fn an_installed_tap_sees_each_chunk_as_it_crossed() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&seen);
        set_wire_tap(Some(Box::new(move |chunk: WireTapChunk<'_>| {
            sink.borrow_mut().push((
                chunk.dir.as_str(),
                chunk.transport,
                chunk.port,
                chunk.bytes.to_vec(),
            ));
        })));
        tap_wire(WireTapDir::Tx, "serial", 3, b"M!{\"id\":1}\n");
        tap_wire(WireTapDir::Rx, "ble", 7, &[0, 1, 2]);
        tap_wire(WireTapDir::Rx, "emu-tab", 7, &[]);
        set_wire_tap(None);
        tap_wire(WireTapDir::Rx, "serial", 3, b"after");
        assert_eq!(
            *seen.borrow(),
            vec![
                ("tx", "serial", 3, b"M!{\"id\":1}\n".to_vec()),
                ("rx", "ble", 7, vec![0, 1, 2]),
            ]
        );
    }
}

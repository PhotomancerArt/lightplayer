//! The wire tap: an opt-in recording of every chunk `/board/<id>/bytes`
//! carries, so a real Studio session's traffic can be sized exactly.
//!
//! Set `LP_EMU_WIRE_TAP=<dir>` before starting `emu serve` (for example
//! `LP_EMU_WIRE_TAP=/tmp/tap just studio-dev-emu`) and each board appends to
//! `<dir>/<board>.tap`. Unset, the tap is `None` and costs one branch per
//! chunk. `scripts/wire-tap/tapstat.py` (`just wire-tap-stat`) reads it.
//!
//! One record per chunk, in the order the pump carried them:
//!
//! ```text
//! <unix_us> <dir> <len>\n<len bytes>\n
//! ```
//!
//! where `<dir>` is `>` for host → board and `<` for board → host. Chunks are
//! whatever the pump read, not lines; the reader reassembles lines.
//!
//! Two properties this instrument keeps:
//!
//! - **It never breaks the pump.** Every failure (the directory is missing,
//!   the disk is full) is swallowed: a tap that could not open records
//!   nothing, and a write that fails is dropped.
//! - **Its timestamps are host wall-clock**, which is right for an edge
//!   instrument and wrong for a gate. Byte sizes from a tap are exact; its
//!   times and rates are the emulator's, and never gated on.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The environment variable that turns the tap on: a directory to write into.
pub const WIRE_TAP_ENV: &str = "LP_EMU_WIRE_TAP";

/// Which way a recorded chunk travelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapDirection {
    /// Host → board (`>`).
    ToBoard,
    /// Board → host (`<`).
    ToHost,
}

impl TapDirection {
    fn marker(self) -> char {
        match self {
            TapDirection::ToBoard => '>',
            TapDirection::ToHost => '<',
        }
    }
}

/// One board's tap file, or nothing when the tap is off.
pub struct WireTap(Option<File>);

impl WireTap {
    /// Open `<$LP_EMU_WIRE_TAP>/<board>.tap` for append, or a no-op tap when
    /// the variable is unset or the file cannot be opened.
    pub fn from_env(board: &str) -> Self {
        match std::env::var_os(WIRE_TAP_ENV) {
            Some(dir) => Self::open_in(Path::new(&dir), board),
            None => Self(None),
        }
    }

    /// Open `<dir>/<board>.tap` for append; a failure yields a no-op tap.
    pub fn open_in(dir: &Path, board: &str) -> Self {
        let path = dir.join(format!("{board}.tap"));
        Self(OpenOptions::new().create(true).append(true).open(path).ok())
    }

    /// Record one chunk with the current wall-clock time.
    pub fn record(&mut self, direction: TapDirection, bytes: &[u8]) {
        let Some(file) = self.0.as_mut() else { return };
        let unix_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros())
            .unwrap_or(0);
        let _ = write_record(file, unix_us, direction, bytes);
    }
}

/// One record, in the tap's format.
fn write_record(
    out: &mut impl Write,
    unix_us: u128,
    direction: TapDirection,
    bytes: &[u8],
) -> std::io::Result<()> {
    writeln!(out, "{unix_us} {} {}", direction.marker(), bytes.len())?;
    out.write_all(bytes)?;
    out.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_header_bytes_newline() {
        let mut out = Vec::new();
        write_record(&mut out, 42, TapDirection::ToHost, b"M!{}\n").unwrap();
        write_record(&mut out, 43, TapDirection::ToBoard, b"ab").unwrap();
        assert_eq!(out, b"42 < 5\nM!{}\n\n43 > 2\nab\n");
    }

    #[test]
    fn a_tap_that_cannot_open_records_nothing_and_does_not_panic() {
        let mut tap = WireTap::open_in(Path::new("/nonexistent/wire-tap-dir"), "c6-a");
        assert!(tap.0.is_none());
        tap.record(TapDirection::ToHost, b"bytes");
    }

    #[test]
    fn a_tap_appends_to_its_board_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut tap = WireTap::open_in(dir.path(), "c6-a");
        tap.record(TapDirection::ToBoard, b"hi");
        drop(tap);
        let body = std::fs::read(dir.path().join("c6-a.tap")).unwrap();
        let text = String::from_utf8(body).unwrap();
        assert!(text.ends_with(" > 2\nhi\n"), "{text:?}");
    }
}

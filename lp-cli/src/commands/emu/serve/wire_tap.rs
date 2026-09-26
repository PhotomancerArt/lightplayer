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
//! A board that was asked to pack (plan `lp-json-pack`) writes learned packed
//! frames (`\n 0x00 'L' COBS 0x00`) into the `<` chunks, recorded as they
//! came. So
//! that a table can set each message's packed bytes beside its JSON ones,
//! the tap also decodes the board's stream as it goes and, after the chunk
//! that completed a packed frame, appends an annotation:
//!
//! ```text
//! <unix_us> P <len> <wire_len>\n<len bytes: M!{json}\n>\n
//! <unix_us> E <len>\n<len bytes: why a packed frame did not decode>\n
//! ```
//!
//! `P` carries the `M!{json}\n` line the frame stands for and the frame's
//! own `wire_len` (`0x00 'L' COBS 0x00`); `E` is a frame that could not be
//! delivered, never dropped silently — torn, or dropped because the tap's
//! learned table lost step with the board's (`desync: …`). The tap decodes
//! with one table per board connection, from its first byte, as a host
//! does: a door reconnect is a new link, and a new tap stream. The `<` chunks stay the exact wire
//! bytes, so a byte count over them is still exact; a reader that wants
//! JSON strips the frames from `<` (they are `0x00`-delimited) and reads the
//! `P` records in their place, which is what `scripts/wire-tap/tapstat.py`
//! does. A tool that knows only `<` and `>` reads the tap after
//! `lp-cli wire unpack --tap`, which rewrites `<` chunks as JSON and drops
//! the annotations.
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

use lpc_wire::{WireChunk, WireForm, WireStream};

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
pub struct WireTap(Option<OpenTap>);

/// A tap that is recording.
struct OpenTap {
    file: File,
    /// The board's stream, decoded as it goes to annotate packed frames.
    to_host: WireStream,
}

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
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Self(file.map(|file| OpenTap {
            file,
            to_host: WireStream::new(),
        }))
    }

    /// Record one chunk with the current wall-clock time, and annotate
    /// every packed frame it completed.
    pub fn record(&mut self, direction: TapDirection, bytes: &[u8]) {
        let Some(tap) = self.0.as_mut() else { return };
        let unix_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros())
            .unwrap_or(0);
        let _ = write_record(&mut tap.file, unix_us, direction, bytes);
        if direction == TapDirection::ToHost {
            let OpenTap { file, to_host } = tap;
            to_host.push(bytes, |chunk| {
                let _ = annotate(file, unix_us, &chunk);
            });
        }
    }
}

/// The annotation for one chunk of the board's stream: `P` for a packed
/// frame, `E` for one that could not be delivered, nothing otherwise (a
/// console line or an `M!` line is already in the `<` chunks as it is).
fn annotate(out: &mut impl Write, unix_us: u128, chunk: &WireChunk) -> std::io::Result<()> {
    match chunk {
        WireChunk::Frame(frame) => {
            let WireForm::Packed { wire_len } = frame.form else {
                return Ok(());
            };
            let line = format!("{}\n", frame.to_line());
            writeln!(out, "{unix_us} P {} {wire_len}", line.len())?;
            out.write_all(line.as_bytes())?;
            out.write_all(b"\n")
        }
        WireChunk::Error(error) => write_error(out, unix_us, error),
        WireChunk::Desync(dropped) => write_error(
            out,
            unix_us,
            &format!(
                "desync: a {} B packed frame dropped: {}",
                dropped.wire_len, dropped.reason
            ),
        ),
        WireChunk::Line(_) => Ok(()),
    }
}

/// An `E` record: a packed frame that could not be delivered, and why.
fn write_error(out: &mut impl Write, unix_us: u128, error: &str) -> std::io::Result<()> {
    writeln!(out, "{unix_us} E {}", error.len())?;
    out.write_all(error.as_bytes())?;
    out.write_all(b"\n")
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

    /// A packed frame is recorded as it came, then annotated with the JSON
    /// line it stands for and its size on the wire, even when it arrives in
    /// two chunks.
    #[test]
    fn a_packed_frame_is_recorded_raw_and_annotated() {
        let message = lpc_wire::WireServerMessage::new(7, lpc_wire::ServerMsgBody::UnloadProject);
        let json = lpc_wire::json::to_string(&message).unwrap();
        let mut framed = vec![0u8; 256];
        let mut table = lpc_wire::LearnedTable::default();
        let n = lpc_wire::ser_learned_frame_to(&mut framed, &mut table, &message).unwrap();
        let framed = &framed[..n];

        let dir = tempfile::tempdir().unwrap();
        let mut tap = WireTap::open_in(dir.path(), "c6-a");
        tap.record(TapDirection::ToHost, &framed[..4]);
        tap.record(TapDirection::ToHost, &framed[4..]);
        drop(tap);

        let body = std::fs::read(dir.path().join("c6-a.tap")).unwrap();
        let line = format!("M!{json}\n");
        let annotation = format!(" P {} {}\n{line}\n", line.len(), n - 1);
        let text = String::from_utf8_lossy(&body);
        assert!(text.ends_with(&annotation), "{text:?}");
        // Both raw chunks are there, byte for byte, before it.
        let raw: Vec<u8> = [&framed[..4], &framed[4..]].concat();
        assert!(
            body.windows(framed.len() - 4).any(|w| w == &raw[4..]),
            "the second chunk is recorded raw"
        );
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

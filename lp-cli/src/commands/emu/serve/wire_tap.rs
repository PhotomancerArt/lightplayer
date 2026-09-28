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
//! A C6 board's USB link is an lp-link (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`): both directions carry
//! frames (`0x00 COBS-FF 0x00`) holding wire messages (JSON, or learned
//! packed replies once the host opted in), and the board's console text runs
//! between them. The chunks are recorded as they came. So that a table can
//! set each message beside its JSON, the tap also reads the link passively as
//! it goes ([`WireLinkSniffer`], one per board connection from its first
//! byte, as a host does) and, after the chunk that completed a message,
//! appends an annotation:
//!
//! ```text
//! <unix_us> P <len> <payload_len>\n<len bytes: M!{json}\n>\n   board → host
//! <unix_us> Q <len> <payload_len>\n<len bytes: M!{json}\n>\n   host → board
//! <unix_us> E <len>\n<len bytes: what could not be read>\n
//! ```
//!
//! `P`/`Q` carry the `M!{json}\n` line a message stands for and its size on
//! the link's proto channel (`payload_len`; the link's own framing,
//! checksums and acknowledgements are not attributed to messages, but the
//! raw chunks still count them); `E` is a message that could not be read or
//! a frame that arrived damaged (the link resent it), never dropped
//! silently. The `<`/`>` chunks stay the exact wire bytes, so a byte count
//! over them is still exact; a reader that wants JSON strips the frames from
//! both directions (they are `0x00`-delimited) and reads the `P`/`Q` records
//! in their place, which is what `scripts/wire-tap/tapstat.py` does. A tool
//! that knows only `<` and `>` reads the tap after `lp-cli wire unpack
//! --tap`, which rewrites both as lines and drops the annotations.
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

use lpc_wire::lp_link::sniffer::Direction;
use lpc_wire::{SniffedWire, WireLinkSniffer};

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
    /// The link, read as it goes to annotate its messages.
    link: WireLinkSniffer,
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
            link: WireLinkSniffer::new(),
        }))
    }

    /// Record one chunk with the current wall-clock time, and annotate
    /// every message it completed.
    pub fn record(&mut self, direction: TapDirection, bytes: &[u8]) {
        let Some(tap) = self.0.as_mut() else { return };
        let unix_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros())
            .unwrap_or(0);
        let _ = write_record(&mut tap.file, unix_us, direction, bytes);
        let dir = match direction {
            TapDirection::ToBoard => Direction::HostToBoard,
            TapDirection::ToHost => Direction::BoardToHost,
        };
        let OpenTap { file, link } = tap;
        link.push(dir, 0, bytes, |item| {
            let _ = annotate(file, unix_us, &item);
        });
    }
}

/// The annotation for one thing read off the link: `P`/`Q` for a message,
/// `E` for what could not be read, nothing for console text (it is in the
/// chunks as it is) or a new session.
fn annotate(out: &mut impl Write, unix_us: u128, item: &SniffedWire) -> std::io::Result<()> {
    match item {
        SniffedWire::Server { payload, .. } => {
            write_message(out, unix_us, 'P', &payload.json, payload.wire_len)
        }
        SniffedWire::Client { json, .. } => write_message(out, unix_us, 'Q', json, json.len()),
        SniffedWire::Unreadable { len, reason, .. } => write_error(
            out,
            unix_us,
            &format!("unreadable: a {len} B message: {reason}"),
        ),
        SniffedWire::Damaged { dir } => write_error(
            out,
            unix_us,
            &format!("damaged: a frame {dir:?} failed its check (the link resent it)"),
        ),
        SniffedWire::Gap { dir, skipped } => write_error(
            out,
            unix_us,
            &format!("gap: {skipped} frame(s) {dir:?} never seen"),
        ),
        SniffedWire::Console { .. } | SniffedWire::Session { .. } => Ok(()),
    }
}

/// A `P` or `Q` record: one message's `M!{json}` line and its payload size.
fn write_message(
    out: &mut impl Write,
    unix_us: u128,
    kind: char,
    json: &str,
    payload_len: usize,
) -> std::io::Result<()> {
    let line = format!("M!{json}\n");
    writeln!(out, "{unix_us} {kind} {} {payload_len}", line.len())?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")
}

/// An `E` record: something on the link that could not be read, and why.
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

    /// Link frames are recorded as they came, then each message is
    /// annotated with the JSON line it stands for and its payload size, in
    /// both directions, however the frames were split into chunks.
    #[test]
    fn link_messages_are_recorded_raw_and_annotated_both_ways() {
        let (to_board, to_host, hello_json) = two_way_session();

        let dir = tempfile::tempdir().unwrap();
        let mut tap = WireTap::open_in(dir.path(), "c6-a");
        for chunk in to_board {
            tap.record(TapDirection::ToBoard, &chunk);
        }
        for chunk in to_host {
            let (a, b) = chunk.split_at(chunk.len() / 2);
            tap.record(TapDirection::ToHost, a);
            tap.record(TapDirection::ToHost, b);
        }
        drop(tap);

        let body = std::fs::read(dir.path().join("c6-a.tap")).unwrap();
        let text = String::from_utf8_lossy(&body);
        let line = format!("M!{hello_json}\n");
        assert!(
            text.contains(&format!(" P {} {}\n{line}\n", line.len(), hello_json.len())),
            "{text:?}"
        );
        let request = "M!{\"id\":3,\"msg\":\"hello\"}\n";
        assert!(
            text.contains(&format!(
                " Q {} {}\n{request}\n",
                request.len(),
                request.len() - 3
            )),
            "{text:?}"
        );
        assert!(!text.contains(" E "), "{text:?}");
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

    /// A host and a board end over a lossless pipe: the handshake, the
    /// board's hello, one request (id 3) and its answer. The frames each end
    /// wrote, in order, and the hello's JSON.
    fn two_way_session() -> (Vec<Vec<u8>>, Vec<Vec<u8>>, String) {
        use lpc_wire::lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
        let mut host: Link<SelectiveRepeat> = Link::new(LinkConfig::usb(), 0x4057_0001);
        let mut board: Link<SelectiveRepeat> = Link::new(LinkConfig::usb(), 0xB0A2_0001);
        let hello = lpc_wire::WireServerMessage::new(7, lpc_wire::ServerMsgBody::UnloadProject);
        let hello_json = lpc_wire::json::to_string(&hello).unwrap();
        let (mut to_board, mut to_host) = (Vec::new(), Vec::new());
        let mut asked = false;
        for step in 0..100u64 {
            let now = step * 1_000;
            while let Some(frame) = host.poll_transmit(now) {
                let frame = frame.to_vec();
                board.on_bytes(now, &frame);
                to_board.push(frame);
            }
            while let Some(event) = board.recv() {
                if let LinkEvent::Up { .. } = event {
                    board.send(CH_PROTO, hello_json.as_bytes()).unwrap();
                }
            }
            while let Some(frame) = board.poll_transmit(now) {
                let frame = frame.to_vec();
                host.on_bytes(now, &frame);
                to_host.push(frame);
            }
            while let Some(event) = host.recv() {
                if matches!(event, LinkEvent::Message { .. }) && !asked {
                    host.send(CH_PROTO, br#"{"id":3,"msg":"hello"}"#).unwrap();
                    asked = true;
                }
            }
        }
        (to_board, to_host, hello_json)
    }
}

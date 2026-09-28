//! `lp-cli wire unpack --lines`: a capture of an `M!`-line link (BLE, the
//! classic ESP32's UART, fw-emu), with packed frames rewritten as the
//! `M!{json}` lines they stand for and every other byte passed through.
//!
//! USB boards speak lp-link since `WIRE_PROTO_VERSION` 30 and are read by
//! [`super::link_unpack`]; the links above keep `M!` lines until their own
//! milestones (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D3).

use std::io::{Read, Write};

use anyhow::{Context, Result};
use lpc_wire::{UnpackEvent, WireUnpacker};

use super::unpack_report::UnpackReport;

/// Rewrite a raw `M!`-line byte stream: every packed frame becomes its
/// `M!{json}\n` line, every other byte passes through.
pub fn unpack_line_stream(
    mut input: impl Read,
    output: &mut impl Write,
    report: &mut UnpackReport,
    log: &mut impl Write,
) -> Result<()> {
    let mut unpacker = WireUnpacker::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut out = Vec::new();
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).context("reading stdin"),
        };
        unpacker.push(&buf[..n], &mut out, |frame| report.note_frame(frame, log));
        output.write_all(&out).context("writing stdout")?;
        out.clear();
    }
    if unpacker.in_frame() {
        report.note_frame(
            UnpackEvent::Dropped("the stream ended inside a packed frame".to_string()),
            log,
        );
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_stream_is_unpacked_byte_for_byte_around_its_frames() {
        let (packed, json_line) = packed_and_json(3);
        let input = [
            b"ESP-ROM:esp32c6\r\n".as_slice(),
            &packed,
            b"\nM!{\"id\":1}\n",
            &packed,
            b"tail without newline",
        ]
        .concat();
        let expected = [
            b"ESP-ROM:esp32c6\r\n".as_slice(),
            json_line.as_bytes(),
            b"\nM!{\"id\":1}\n",
            json_line.as_bytes(),
            b"tail without newline",
        ]
        .concat();

        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(true);
        unpack_line_stream(input.as_slice(), &mut out, &mut report, &mut log).unwrap();
        report.finish(&mut log).unwrap();

        assert_eq!(out, expected);
        let wire = packed.len() - 1;
        let json = json_line.len() - 1;
        assert_eq!(
            String::from_utf8(log).unwrap(),
            format!(
                "frame 1 packed {wire} json {json}\nframe 2 packed {wire} json {json}\n\
                 total frames 2 packed {} json {} unreadable 0 errors 0\n",
                2 * wire,
                2 * json
            )
        );
    }

    #[test]
    fn a_torn_frame_is_reported_not_swallowed() {
        let (packed, _) = packed_and_json(3);
        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(false);
        unpack_line_stream(&packed[..6], &mut out, &mut report, &mut log).unwrap();
        assert_eq!(report.errors(), 1);
        assert!(
            String::from_utf8(log).unwrap().contains("ended inside"),
            "the torn tail is named"
        );
    }

    /// `\n 0x00 'L' COBS 0x00` for a message, coded as the first frame after
    /// a table reset (so any reader takes it), and the `\nM!{json}\n` it
    /// stands for (the leading `\n` is the firmware's, and passes through).
    pub(crate) fn packed_and_json(id: u64) -> (Vec<u8>, String) {
        let message = lpc_wire::WireServerMessage::new(
            id,
            lpc_wire::ServerMsgBody::Log {
                level: lpc_wire::server::api::LogLevel::Info,
                message: "a\nb".to_string(),
            },
        );
        let mut framed = vec![0u8; 512];
        let mut table = lpc_wire::LearnedTable::default();
        let n = lpc_wire::ser_learned_frame_to(&mut framed, &mut table, &message).unwrap();
        framed.truncate(n);
        let json = lpc_wire::json::to_string(&message).unwrap();
        (framed, format!("\nM!{json}\n"))
    }
}

//! `lp-cli wire unpack`: a capture of a board's lp-link, rewritten as the
//! lines a person reads.
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB link is an lp-link (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D10): frames, checksums,
//! resends, channels. A capture is read passively ([`WireLinkSniffer`]):
//! each wire message comes out as the `M!{json}` line it would have been
//! (packed or not), each console line (log record or raw text) as itself,
//! and a message that cannot be read as an `<unreadable message …>` line.
//! Resends are deduplicated, and damaged frames (which the link resent) are
//! counted, not written.

use std::io::{Read, Write};

use anyhow::{Context, Result};
use lpc_wire::lp_link::sniffer::Direction;
use lpc_wire::{SniffedWire, WireLinkSniffer};

use super::unpack_report::UnpackReport;

/// Rewrite a raw capture of one direction of a link (`dir`; a board's
/// output is [`Direction::BoardToHost`]) as lines.
pub fn unpack_link_stream(
    mut input: impl Read,
    dir: Direction,
    output: &mut impl Write,
    report: &mut UnpackReport,
    log: &mut impl Write,
) -> Result<()> {
    let mut sniffer = WireLinkSniffer::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut out = Vec::new();
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).context("reading stdin"),
        };
        sniffer.push(dir, 0, &buf[..n], |item| {
            report.note_link(&item, log);
            out.extend_from_slice(rendered(&item).as_bytes());
        });
        output.write_all(&out).context("writing stdout")?;
        out.clear();
    }
    sniffer.flush(|item| {
        report.note_link(&item, log);
        out.extend_from_slice(rendered(&item).as_bytes());
    });
    output.write_all(&out).context("writing stdout")
}

/// The text one sniffed item stands for, newline included; empty for what a
/// reader of lines has no use for (sessions, damaged frames, gaps — the
/// report counts those).
pub fn rendered(item: &SniffedWire) -> String {
    match item {
        SniffedWire::Server { payload, .. } => format!("M!{}\n", payload.json),
        SniffedWire::Client { json, .. } => format!("M!{json}\n"),
        SniffedWire::Console { line, .. } => format!("{line}\n"),
        SniffedWire::Unreadable { len, reason, .. } => {
            format!("<unreadable message: {len} bytes, {reason}>\n")
        }
        SniffedWire::Session { .. } | SniffedWire::Damaged { .. } | SniffedWire::Gap { .. } => {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::wire::test_capture::{capture, log_reply};

    #[test]
    fn a_board_capture_reads_as_its_console_lines_and_messages() {
        let session = capture(
            "ESP-ROM:esp32c6\r\n[INIT] booting\n",
            &[log_reply(5)],
            false,
        );

        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(true);
        unpack_link_stream(
            session.to_host().as_slice(),
            Direction::BoardToHost,
            &mut out,
            &mut report,
            &mut log,
        )
        .unwrap();
        report.finish(&mut log).unwrap();

        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "ESP-ROM:esp32c6");
        assert_eq!(lines[1], "[INIT] booting");
        assert!(
            lines[2].starts_with("M!{") && lines[2].contains("\"hello\""),
            "{text}"
        );
        let reply = lpc_wire::json::to_string(&log_reply(5)).unwrap();
        assert!(lines.contains(&format!("M!{reply}").as_str()), "{text}");
        let log = String::from_utf8(log).unwrap();
        assert!(log.contains("total messages 3 packed 0"), "{log}");
    }

    /// A classic's capture (plan `classic-uart-on-lp-link`, P3): its UART0
    /// link is `LinkConfig::uart()` on the host and the board's cut of it
    /// (four-frame windows, a 200 ms resend floor, SYN backoff), framed as
    /// USB is, so the one sniffer reads it with nothing chip-specific: the
    /// ROM's boot text, packed replies, and the panic path's `0xFF` text mark
    /// after the link's last frame.
    #[test]
    fn a_classic_uart_capture_reads_like_a_usb_one() {
        use lpc_wire::lp_link::LinkConfig;

        use crate::commands::wire::test_capture::capture_over;

        let board = LinkConfig {
            min_rto: 200_000,
            syn_backoff: 4,
            ..LinkConfig::uart()
        };
        let replies: Vec<_> = (30..40).map(log_reply).collect();
        let session = capture_over(
            LinkConfig::uart(),
            board,
            "ets Jun  8 2016 00:22:57\r\n\r\nrst:0x1 (POWERON_RESET),boot:0x13 \
             (SPI_FAST_FLASH_BOOT)\r\n[INIT] fw-esp32v3 boot\r\n",
            &replies,
            true,
        );
        let mut bytes = session.to_host();
        bytes.extend_from_slice(b"\xFF\r\n[PANIC] after the link's last frame\r\n");

        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(true);
        unpack_link_stream(
            bytes.as_slice(),
            Direction::BoardToHost,
            &mut out,
            &mut report,
            &mut log,
        )
        .unwrap();
        report.finish(&mut log).unwrap();

        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "ets Jun  8 2016 00:22:57", "{text}");
        assert!(lines.contains(&"rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)"));
        assert!(lines.contains(&"[INIT] fw-esp32v3 boot"), "{text}");
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("M!{") && l.contains("\"hello\"")),
            "{text}"
        );
        for reply in &replies {
            let json = lpc_wire::json::to_string(reply).unwrap();
            assert!(text.contains(&format!("M!{json}\n")), "{text}");
        }
        assert!(
            lines.contains(&"[PANIC] after the link's last frame"),
            "{text}"
        );
        let log = String::from_utf8(log).unwrap();
        assert!(!log.contains("packed 0 "), "the replies went packed: {log}");
        assert!(log.contains("unreadable 0 damaged 0 gaps 0"), "{log}");
    }

    /// Packed replies read back as the JSON they stand for, from the
    /// capture's own session start, and are sized as packed.
    #[test]
    fn packed_replies_read_as_their_json() {
        let replies: Vec<_> = (10..20).map(log_reply).collect();
        let session = capture("", &replies, true);

        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(true);
        unpack_link_stream(
            session.to_host().as_slice(),
            Direction::BoardToHost,
            &mut out,
            &mut report,
            &mut log,
        )
        .unwrap();
        report.finish(&mut log).unwrap();

        let text = String::from_utf8(out).unwrap();
        for reply in &replies {
            let json = lpc_wire::json::to_string(reply).unwrap();
            assert!(text.contains(&format!("M!{json}\n")), "{text}");
        }
        let log = String::from_utf8(log).unwrap();
        assert!(log.contains(" packed "), "{log}");
        assert!(!log.contains("packed 0 "), "the replies went packed: {log}");
        assert!(log.contains("unreadable 0 damaged 0 gaps 0"), "{log}");
    }

    /// A capture of the host's side reads as its requests.
    #[test]
    fn a_host_capture_reads_as_its_requests() {
        let session = capture("", &[], false);

        let mut out = Vec::new();
        let mut report = UnpackReport::new(false);
        unpack_link_stream(
            session.to_board().as_slice(),
            Direction::HostToBoard,
            &mut out,
            &mut report,
            &mut Vec::new(),
        )
        .unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "M!{\"id\":1,\"msg\":\"hello\"}\n"
        );
    }
}

//! `lp-cli wire unpack --tap`: a wire tap (`LP_EMU_WIRE_TAP`) rewritten as
//! the tap of a link with no framing.
//!
//! The tap format is `lp-cli/src/commands/emu/serve/wire_tap.rs`'s: one
//! `<unix_us> <dir> <len>\n<len bytes>\n` record per chunk the door carried,
//! plus its annotations. Both directions go through one
//! [`WireLinkSniffer`] for the whole tap (they are one lp-link, and a frame
//! spans chunks); after each chunk, what it completed is written back as a
//! record of lines — messages as `M!{json}`, console text as itself — in
//! the direction it went. The annotations are dropped, since the records now
//! say the same thing.

use std::io::{Read, Write};

use anyhow::{Context, Result, bail};
use lpc_wire::WireLinkSniffer;
use lpc_wire::lp_link::sniffer::Direction;

use super::link_unpack::rendered;
use super::unpack_report::UnpackReport;

/// Rewrite the tap on `input` onto `output`.
pub fn unpack_tap(
    mut input: impl Read,
    output: &mut impl Write,
    report: &mut UnpackReport,
    log: &mut impl Write,
) -> Result<()> {
    let mut data = Vec::new();
    input.read_to_end(&mut data).context("reading stdin")?;
    let mut sniffer = WireLinkSniffer::new();
    let mut at = 0;
    while at < data.len() {
        let record = read_record(&data, at)
            .with_context(|| format!("a tap record at byte {at} is malformed"))?;
        at = record.next;
        let dir = match record.direction {
            "<" => Direction::BoardToHost,
            ">" => Direction::HostToBoard,
            // The tap's own annotations (messages, errors): the lines below
            // say the same.
            annotation if annotation.len() == 1 && annotation.chars().all(char::is_uppercase) => {
                continue;
            }
            other => bail!("a tap record has an unknown direction {other:?}"),
        };
        let mut lines: Vec<(Direction, String)> = Vec::new();
        sniffer.push(dir, 0, record.bytes, |item| {
            report.note_link(&item, log);
            let text = rendered(&item);
            if !text.is_empty() {
                lines.push((item_direction(&item).unwrap_or(dir), text));
            }
        });
        write_lines(output, record.unix_us, lines)?;
    }
    let mut lines = Vec::new();
    sniffer.flush(|item| {
        report.note_link(&item, log);
        let text = rendered(&item);
        if !text.is_empty() {
            lines.push((
                item_direction(&item).unwrap_or(Direction::BoardToHost),
                text,
            ));
        }
    });
    write_lines(output, "0", lines)
}

/// The direction a sniffed item went, when it names one.
fn item_direction(item: &lpc_wire::SniffedWire) -> Option<Direction> {
    use lpc_wire::SniffedWire as S;
    match item {
        S::Server { .. } => Some(Direction::BoardToHost),
        S::Client { .. } => Some(Direction::HostToBoard),
        S::Console { dir, .. }
        | S::Unreadable { dir, .. }
        | S::Session { dir, .. }
        | S::Damaged { dir }
        | S::Gap { dir, .. }
        | S::Sealed { dir, .. } => Some(*dir),
    }
}

/// Write `lines` as records, one per run of the same direction.
fn write_lines(
    output: &mut impl Write,
    unix_us: &str,
    lines: Vec<(Direction, String)>,
) -> Result<()> {
    let mut run: Option<(Direction, Vec<u8>)> = None;
    for (dir, text) in lines {
        match &mut run {
            Some((current, bytes)) if *current == dir => bytes.extend_from_slice(text.as_bytes()),
            _ => {
                if let Some((current, bytes)) = run.take() {
                    write_record(output, unix_us, marker(current), &bytes)?;
                }
                run = Some((dir, text.into_bytes()));
            }
        }
    }
    if let Some((current, bytes)) = run {
        write_record(output, unix_us, marker(current), &bytes)?;
    }
    Ok(())
}

fn marker(dir: Direction) -> &'static str {
    match dir {
        Direction::BoardToHost => "<",
        Direction::HostToBoard => ">",
    }
}

/// One record, borrowed from the tap.
struct TapRecord<'a> {
    unix_us: &'a str,
    direction: &'a str,
    bytes: &'a [u8],
    /// Where the next record starts.
    next: usize,
}

fn read_record(data: &[u8], at: usize) -> Result<TapRecord<'_>> {
    let Some(nl) = data[at..].iter().position(|&b| b == b'\n') else {
        bail!("no header line");
    };
    let header = std::str::from_utf8(&data[at..at + nl]).context("the header is not text")?;
    // `<unix_us> <dir> <len>`, and an annotation may add fields.
    let mut fields = header.split(' ');
    let (Some(unix_us), Some(direction), Some(len)) = (fields.next(), fields.next(), fields.next())
    else {
        bail!("the header {header:?} has too few fields");
    };
    let len: usize = len
        .parse()
        .with_context(|| format!("the length in {header:?}"))?;
    let start = at + nl + 1;
    let end = start + len;
    if data.get(end) != Some(&b'\n') {
        bail!("the record {header:?} is cut short");
    }
    Ok(TapRecord {
        unix_us,
        direction,
        bytes: &data[start..end],
        next: end + 1,
    })
}

fn write_record(out: &mut impl Write, unix_us: &str, direction: &str, bytes: &[u8]) -> Result<()> {
    writeln!(out, "{unix_us} {direction} {}", bytes.len()).context("writing stdout")?;
    out.write_all(bytes).context("writing stdout")?;
    out.write_all(b"\n").context("writing stdout")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::wire::test_capture::{capture, log_reply};

    #[test]
    fn a_tap_reads_as_lines_both_ways_with_its_annotations_dropped() {
        let session = capture("boot ok\n", &[log_reply(4)], true);
        let mut tap = Vec::new();
        for (n, (dir, bytes)) in session.chunks.iter().enumerate() {
            record(
                &mut tap,
                &format!("{n} {} {}", marker(*dir), bytes.len()),
                bytes,
            );
        }
        record(&mut tap, "999 P 3 9", b"M!x");

        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(false);
        unpack_tap(tap.as_slice(), &mut out, &mut report, &mut log).unwrap();

        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("0 < 8\nboot ok\n\n"), "{text}");
        assert!(
            text.contains(" > 25\nM!{\"id\":1,\"msg\":\"hello\"}\n\n"),
            "the host's request, as a line: {text}"
        );
        let reply = lpc_wire::json::to_string(&log_reply(4)).unwrap();
        assert!(text.contains(&format!("M!{reply}\n")), "{text}");
        assert!(!text.contains("M!x"), "annotations are dropped: {text}");
        assert!(log.is_empty(), "{}", String::from_utf8_lossy(&log));
    }

    #[test]
    fn a_cut_short_record_is_an_error() {
        let mut out = Vec::new();
        let mut report = UnpackReport::new(false);
        let error = unpack_tap(
            b"1 < 10\nabc".as_slice(),
            &mut out,
            &mut report,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("cut short"), "{error:#}");
    }

    fn record(out: &mut Vec<u8>, header: &str, bytes: &[u8]) {
        out.extend_from_slice(header.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(bytes);
        out.push(b'\n');
    }
}

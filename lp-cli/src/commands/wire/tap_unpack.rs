//! `lp-cli wire unpack --tap`: a wire tap (`LP_EMU_WIRE_TAP`) rewritten as
//! the tap of a link that never packed.
//!
//! The tap format is `lp-cli/src/commands/emu/serve/wire_tap.rs`'s: one
//! `<unix_us> <dir> <len>\n<len bytes>\n` record per chunk the door carried,
//! plus its `P`/`E` annotations of packed frames. Board → host chunks (`<`)
//! go through one [`WireUnpacker`] for the whole tap (a frame spans chunks),
//! and each is written back with its new length; host → board chunks (`>`)
//! pass through; the annotations are dropped, since the `<` chunks now say
//! the same thing in JSON.

use std::io::{Read, Write};

use anyhow::{Context, Result, bail};
use lpc_wire::WireUnpacker;

use super::handler::UnpackReport;

/// Rewrite the tap on `input` onto `output`.
pub fn unpack_tap(
    mut input: impl Read,
    output: &mut impl Write,
    report: &mut UnpackReport,
    log: &mut impl Write,
) -> Result<()> {
    let mut data = Vec::new();
    input.read_to_end(&mut data).context("reading stdin")?;
    let mut to_host = WireUnpacker::new();
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let record = read_record(&data, at)
            .with_context(|| format!("a tap record at byte {at} is malformed"))?;
        at = record.next;
        match record.direction {
            "<" => {
                to_host.push(record.bytes, &mut out, |frame| report.note(frame, log));
                if !out.is_empty() {
                    write_record(output, record.unix_us, "<", &out)?;
                    out.clear();
                }
            }
            ">" => write_record(output, record.unix_us, ">", record.bytes)?,
            "P" | "E" => {}
            other => bail!("a tap record has an unknown direction {other:?}"),
        }
    }
    if to_host.in_frame() {
        report.note(
            lpc_wire::UnpackEvent::Dropped("the tap ended inside a packed frame".to_string()),
            log,
        );
    }
    Ok(())
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
    // `<unix_us> <dir> <len>`, and a `P` annotation adds `<wire_len>`.
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
    use crate::commands::wire::handler::tests::packed_and_json;

    #[test]
    fn a_tap_is_rewritten_as_json_with_its_annotations_dropped() {
        let (packed, json_line) = packed_and_json(4);
        let (head, tail) = packed.split_at(5);
        let mut tap = Vec::new();
        record(&mut tap, "1 > 3", b"M!x");
        record(&mut tap, "2 < 5", head);
        record(&mut tap, &format!("3 < {}", tail.len()), tail);
        let annotation = &json_line.as_bytes()[1..];
        record(
            &mut tap,
            &format!("3 P {} {}", annotation.len(), packed.len() - 1),
            annotation,
        );
        record(&mut tap, "4 < 3", b"ok\n");

        let mut out = Vec::new();
        let mut log = Vec::new();
        let mut report = UnpackReport::new(false);
        unpack_tap(tap.as_slice(), &mut out, &mut report, &mut log).unwrap();

        // The frame's `\n` and first bytes arrive in record 2: the `\n`
        // passes through there, the JSON line comes out with record 3.
        let mut expected = Vec::new();
        record(&mut expected, "1 > 3", b"M!x");
        record(&mut expected, "2 < 1", b"\n");
        let line = &json_line.as_bytes()[1..];
        record(&mut expected, &format!("3 < {}", line.len()), line);
        record(&mut expected, "4 < 3", b"ok\n");
        assert_eq!(
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&expected)
        );
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

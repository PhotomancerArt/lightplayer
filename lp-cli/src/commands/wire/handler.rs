//! `lp-cli wire unpack`: packed frames back to `M!{json}` lines.

use std::io::{Read, Write};

use anyhow::{Context, Result};
use lpc_wire::{UnpackEvent, WireUnpacker};

use super::args::{UnpackArgs, WireCli, WireSubcommand};
use super::tap_unpack::unpack_tap;

/// Run `lp-cli wire …`.
pub fn handle_wire(cli: WireCli) -> Result<()> {
    match cli.subcommand {
        WireSubcommand::Unpack(args) => handle_unpack(&args),
    }
}

fn handle_unpack(args: &UnpackArgs) -> Result<()> {
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let mut report = UnpackReport::new(args.sizes);
    if args.tap {
        unpack_tap(stdin, &mut stdout, &mut report, &mut stderr)?;
    } else {
        unpack_stream(stdin, &mut stdout, &mut report, &mut stderr)?;
    }
    stdout.flush().context("writing stdout")?;
    report.finish(&mut stderr)?;
    Ok(())
}

/// Rewrite a raw byte stream (a serial capture, a pty log): every packed
/// frame becomes its `M!{json}\n` line, every other byte passes through.
pub fn unpack_stream(
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
        unpacker.push(&buf[..n], &mut out, |frame| report.note(frame, log));
        output.write_all(&out).context("writing stdout")?;
        out.clear();
    }
    if unpacker.in_frame() {
        report.note(
            UnpackEvent::Dropped("the stream ended inside a packed frame".to_string()),
            log,
        );
    }
    Ok(())
}

/// What `wire unpack` saw: packed frames, their sizes, the frames it could
/// not read (a capture that starts mid-connection), and the frames it could
/// not deliver. Errors always reach stderr; per-frame sizes only with
/// `--sizes`.
pub struct UnpackReport {
    sizes: bool,
    frames: usize,
    packed_bytes: usize,
    json_bytes: usize,
    unreadable: usize,
    errors: usize,
}

impl UnpackReport {
    pub fn new(sizes: bool) -> Self {
        Self {
            sizes,
            frames: 0,
            packed_bytes: 0,
            json_bytes: 0,
            unreadable: 0,
            errors: 0,
        }
    }

    /// What happened to one packed frame.
    pub fn note(&mut self, event: UnpackEvent, log: &mut impl Write) {
        match event {
            UnpackEvent::Unpacked(frame) => {
                self.frames += 1;
                self.packed_bytes += frame.wire_len;
                self.json_bytes += frame.json_line_len;
                if self.sizes {
                    let _ = writeln!(
                        log,
                        "frame {} packed {} json {}",
                        self.frames, frame.wire_len, frame.json_line_len
                    );
                }
            }
            UnpackEvent::Unreadable(frame) => {
                self.unreadable += 1;
                if self.unreadable == 1 {
                    let _ = writeln!(
                        log,
                        "wire unpack: learned frames before the board's next table reset cannot \
                         be read (the capture starts mid-connection, or a frame before them was \
                         lost); each is written as a `<learned frame: table unknown …>` line"
                    );
                }
                if self.sizes {
                    let _ = writeln!(
                        log,
                        "unreadable {} packed {} ({})",
                        self.unreadable, frame.wire_len, frame.reason
                    );
                }
            }
            UnpackEvent::Dropped(error) => {
                self.errors += 1;
                let _ = writeln!(log, "wire unpack: {error}");
            }
        }
    }

    /// The closing total (with `--sizes`).
    pub fn finish(&self, log: &mut impl Write) -> Result<()> {
        if self.sizes {
            writeln!(
                log,
                "total frames {} packed {} json {} unreadable {} errors {}",
                self.frames, self.packed_bytes, self.json_bytes, self.unreadable, self.errors
            )
            .context("writing stderr")?;
        }
        Ok(())
    }
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
        unpack_stream(input.as_slice(), &mut out, &mut report, &mut log).unwrap();
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
        unpack_stream(&packed[..6], &mut out, &mut report, &mut log).unwrap();
        assert_eq!(report.errors, 1);
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

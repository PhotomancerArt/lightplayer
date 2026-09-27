//! What `lp-cli wire unpack` saw, for stderr: messages and their sizes, what
//! it could not read, and what the link itself recovered from. Errors always
//! reach stderr; per-message sizes only with `--sizes`.

use std::io::Write;

use anyhow::{Context, Result};
use lpc_wire::lp_link::sniffer::Direction;
use lpc_wire::{SniffedWire, UnpackEvent};

/// The running tally. See the module docs.
pub struct UnpackReport {
    sizes: bool,
    /// Messages read (lp-link), or packed frames (`M!` lines).
    frames: usize,
    packed: usize,
    /// Bytes on the wire: packed frames (`M!` lines), or proto payloads
    /// (lp-link).
    wire_bytes: usize,
    json_bytes: usize,
    unreadable: usize,
    errors: usize,
    damaged: usize,
    gaps: usize,
    unverified: usize,
    sessions: usize,
    /// Whether this report is of an lp-link capture (decides the total's
    /// words).
    link: bool,
}

impl UnpackReport {
    pub fn new(sizes: bool) -> Self {
        Self {
            sizes,
            frames: 0,
            packed: 0,
            wire_bytes: 0,
            json_bytes: 0,
            unreadable: 0,
            errors: 0,
            damaged: 0,
            gaps: 0,
            unverified: 0,
            sessions: 0,
            link: false,
        }
    }

    /// Frames that could not be delivered.
    pub fn errors(&self) -> usize {
        self.errors
    }

    /// What happened to one packed frame of an `M!`-line capture.
    pub fn note_frame(&mut self, event: UnpackEvent, log: &mut impl Write) {
        match event {
            UnpackEvent::Unpacked(frame) => {
                self.frames += 1;
                self.packed += 1;
                self.wire_bytes += frame.wire_len;
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

    /// One thing read off an lp-link capture.
    pub fn note_link(&mut self, item: &SniffedWire, log: &mut impl Write) {
        self.link = true;
        match item {
            SniffedWire::Server { payload, verified } => {
                let line = payload.json.len() + "M!\n".len();
                self.message(payload.packed, payload.wire_len, line, *verified, log);
            }
            SniffedWire::Client { json, verified } => {
                let line = json.len() + "M!\n".len();
                self.message(false, json.len(), line, *verified, log);
            }
            SniffedWire::Session { dir, nonce } => {
                self.sessions += 1;
                if self.sizes {
                    let _ = writeln!(log, "session {} nonce {nonce:#010x}", dir_word(*dir));
                }
            }
            SniffedWire::Unreadable { dir, len, reason } => {
                self.unreadable += 1;
                if self.unreadable == 1 {
                    let _ = writeln!(
                        log,
                        "wire unpack: packed messages before the capture's first link session \
                         cannot be read (it starts mid-session); each is written as an \
                         `<unreadable message …>` line"
                    );
                }
                if self.sizes {
                    let _ = writeln!(
                        log,
                        "unreadable {} {} {len} ({reason})",
                        self.unreadable,
                        dir_word(*dir)
                    );
                }
            }
            SniffedWire::Damaged { dir } => {
                self.damaged += 1;
                if self.sizes {
                    let _ = writeln!(log, "damaged frame {} (the link resent it)", dir_word(*dir));
                }
            }
            SniffedWire::Gap { dir, skipped } => {
                self.gaps += 1;
                let _ = writeln!(
                    log,
                    "wire unpack: {skipped} frame(s) {} missing from the capture",
                    dir_word(*dir)
                );
            }
            SniffedWire::Console { .. } => {}
        }
    }

    fn message(
        &mut self,
        packed: bool,
        wire_len: usize,
        line_len: usize,
        verified: bool,
        log: &mut impl Write,
    ) {
        self.frames += 1;
        self.wire_bytes += wire_len;
        self.json_bytes += line_len;
        if packed {
            self.packed += 1;
        }
        if !verified {
            self.unverified += 1;
        }
        if self.sizes {
            let form = if packed { "packed" } else { "json" };
            let _ = writeln!(
                log,
                "message {} {form} {wire_len} json {line_len}",
                self.frames
            );
        }
    }

    /// The closing total (with `--sizes`), and what the link recovered from.
    pub fn finish(&self, log: &mut impl Write) -> Result<()> {
        if self.link && self.unverified > 0 {
            writeln!(
                log,
                "wire unpack: {} message(s) read before the capture's first handshake were \
                 not checksum-verified",
                self.unverified
            )
            .context("writing stderr")?;
        }
        if !self.sizes {
            return Ok(());
        }
        if self.link {
            writeln!(
                log,
                "total messages {} packed {} payload {} json {} unreadable {} damaged {} gaps {} \
                 sessions {}",
                self.frames,
                self.packed,
                self.wire_bytes,
                self.json_bytes,
                self.unreadable,
                self.damaged,
                self.gaps,
                self.sessions
            )
        } else {
            writeln!(
                log,
                "total frames {} packed {} json {} unreadable {} errors {}",
                self.frames, self.wire_bytes, self.json_bytes, self.unreadable, self.errors
            )
        }
        .context("writing stderr")
    }
}

fn dir_word(dir: Direction) -> &'static str {
    match dir {
        Direction::BoardToHost => "board->host",
        Direction::HostToBoard => "host->board",
    }
}

//! Host → board: re-joining a radio link's written chunks into wire lines.
//!
//! A radio link (BLE's Nordic-UART RX characteristic) delivers the host's
//! bytes in writes of at most one ATT value each, and a `M!{json}\n` line
//! routinely spans several. This is the USB io task's `process_read_buffer`
//! for such a link: bytes go in, complete lines come out, and only `M!` lines
//! are kept — anything else on the line (a stray newline, a lab's text
//! command) is not a wire frame and is ignored, exactly as on USB.
//!
//! Unlike USB, a line is capped. A peer that never sends a newline must not
//! grow a heap buffer without bound on a device whose heap is the product's
//! render budget, so a line longer than [`LineJoiner::new`]'s cap is dropped
//! whole: the joiner discards through the next newline and reports the drop,
//! and the line after it parses normally.

use alloc::string::String;
use alloc::vec::Vec;

/// What one [`LineJoiner::push`] call saw besides the lines it delivered.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct JoinReport {
    /// Complete non-`M!` lines ignored.
    pub ignored_lines: u32,
    /// Lines dropped because they outgrew the cap (counted once each, when
    /// the overflow is detected).
    pub overflowed_lines: u32,
    /// `M!` lines dropped because they were not UTF-8.
    pub invalid_utf8_lines: u32,
}

impl JoinReport {
    /// Anything worth a log line (ignored non-wire lines are not).
    #[must_use]
    pub fn dropped_any(&self) -> bool {
        self.overflowed_lines > 0 || self.invalid_utf8_lines > 0
    }
}

/// Re-joins chunked bytes into `M!` lines, capped at a maximum line length.
pub struct LineJoiner {
    buf: Vec<u8>,
    cap: usize,
    /// Discarding the rest of an over-long line, up to its newline.
    discarding: bool,
}

impl LineJoiner {
    /// A joiner that drops any line (excluding its newline) longer than
    /// `cap` bytes.
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            buf: Vec::new(),
            cap,
            discarding: false,
        }
    }

    /// Feed one written chunk; `on_line` receives each complete `M!` line
    /// without its trailing newline (and without a trailing `\r`).
    pub fn push(&mut self, bytes: &[u8], mut on_line: impl FnMut(String)) -> JoinReport {
        let mut report = JoinReport::default();
        let mut rest = bytes;
        while !rest.is_empty() {
            let newline = rest.iter().position(|&b| b == b'\n');
            let (segment, ended) = match newline {
                Some(at) => (&rest[..at], true),
                None => (rest, false),
            };
            rest = match newline {
                Some(at) => &rest[at + 1..],
                None => &[],
            };

            if self.discarding {
                if ended {
                    self.discarding = false;
                }
                continue;
            }
            if self.buf.len() + segment.len() > self.cap {
                report.overflowed_lines += 1;
                self.buf = Vec::new();
                self.discarding = !ended;
                continue;
            }
            self.buf.extend_from_slice(segment);
            if !ended {
                continue;
            }

            let mut line = core::mem::take(&mut self.buf);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !line.starts_with(b"M!") {
                if !line.is_empty() {
                    report.ignored_lines += 1;
                }
                continue;
            }
            match String::from_utf8(line) {
                Ok(line) => on_line(line),
                Err(_) => report.invalid_utf8_lines += 1,
            }
        }
        report
    }

    /// Bytes held for a line not yet ended.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn a_line_split_across_chunks_comes_out_whole() {
        let mut joiner = LineJoiner::new(64);
        let mut lines = Vec::new();
        joiner.push(b"M!{\"id\":", |l| lines.push(l));
        assert!(lines.is_empty());
        joiner.push(b"1}\nM!{\"id\"", |l| lines.push(l));
        joiner.push(b":2}\n", |l| lines.push(l));
        assert_eq!(lines, vec!["M!{\"id\":1}", "M!{\"id\":2}"]);
        assert_eq!(joiner.pending_len(), 0);
    }

    #[test]
    fn non_wire_lines_are_ignored_and_counted() {
        let mut joiner = LineJoiner::new(64);
        let mut lines = Vec::new();
        let report = joiner.push(b"\nparams\nM!{}\r\n", |l| lines.push(l));
        assert_eq!(lines, vec!["M!{}"]);
        assert_eq!(report.ignored_lines, 1);
        assert!(!report.dropped_any());
    }

    #[test]
    fn an_overlong_line_is_dropped_and_the_next_one_parses() {
        let mut joiner = LineJoiner::new(8);
        let mut lines = Vec::new();
        let first = joiner.push(b"M!0123456", |l| lines.push(l));
        assert_eq!(first.overflowed_lines, 1);
        assert!(first.dropped_any());
        // The rest of the long line is discarded through its newline.
        let second = joiner.push(b"789abc\nM!ok\n", |l| lines.push(l));
        assert_eq!(second.overflowed_lines, 0);
        assert_eq!(lines, vec!["M!ok"]);
    }

    #[test]
    fn a_line_exactly_at_the_cap_is_kept() {
        let mut joiner = LineJoiner::new(4);
        let mut lines = Vec::new();
        let report = joiner.push(b"M!ab\n", |l| lines.push(l));
        assert_eq!(report, JoinReport::default());
        assert_eq!(lines, vec!["M!ab"]);
    }

    #[test]
    fn invalid_utf8_is_dropped_not_delivered() {
        let mut joiner = LineJoiner::new(16);
        let mut lines = Vec::new();
        let report = joiner.push(b"M!\xff\xfe\nM!x\n", |l| lines.push(l));
        assert_eq!(report.invalid_utf8_lines, 1);
        assert_eq!(lines, vec!["M!x"]);
    }
}

//! How a device link's console output becomes the lines hosts show.
//!
//! A board on lp-link says console things two ways: **log records** on the
//! link's log channel (`level ‖ text`, [`lp_link::LogRing`]), and **raw text**
//! outside frames (the ROM banner, early boot, the boot marker, a panic).
//! Hosts show both as the lines the board's logger always printed —
//! `[LEVEL] target: text` for a record, the text itself for raw output — so
//! Studio's console, `lp-cli` and the tools read the same thing they read
//! when the board printed `M!`-era text. [`WireLinkPort`](crate::WireLinkPort)
//! and [`WireLinkSniffer`](crate::WireLinkSniffer) both go through here.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_link::log_ring::{LEVEL_DEBUG, LEVEL_ERROR, LEVEL_INFO, LEVEL_TRACE, LEVEL_WARN};

/// One log-channel record as console lines (`[LEVEL] text`, one per line of
/// the record). Level 0, the link's own notices ("n log records dropped"),
/// reads `[LINK]`.
pub fn log_record_lines(record: &[u8]) -> Vec<String> {
    let Some((&level, text)) = record.split_first() else {
        return Vec::new();
    };
    let name = match level {
        LEVEL_ERROR => "ERROR",
        LEVEL_WARN => "WARN",
        LEVEL_INFO => "INFO",
        LEVEL_DEBUG => "DEBUG",
        LEVEL_TRACE => "TRACE",
        _ => "LINK",
    };
    let text = String::from_utf8_lossy(text);
    text.trim_end_matches(['\r', '\n'])
        .split('\n')
        .map(|line| format!("[{name}] {}", line.trim_end_matches('\r')))
        .collect()
}

/// Raw text outside frames, split into lines as it arrives in chunks.
#[derive(Debug, Default)]
pub struct TextLines {
    /// Text since the last newline.
    partial: Vec<u8>,
}

impl TextLines {
    pub const fn new() -> Self {
        TextLines {
            partial: Vec::new(),
        }
    }

    /// Append `bytes` and call `on` for every line they complete (without
    /// its `\n` or a trailing `\r`).
    pub fn push(&mut self, bytes: &[u8], mut on: impl FnMut(String)) {
        self.partial.extend_from_slice(bytes);
        let mut start = 0;
        while let Some(nl) = self.partial[start..].iter().position(|&b| b == b'\n') {
            let mut line = &self.partial[start..start + nl];
            if let Some(stripped) = line.strip_suffix(b"\r") {
                line = stripped;
            }
            on(String::from_utf8_lossy(line).into_owned());
            start += nl + 1;
        }
        self.partial.drain(..start);
    }

    /// The unterminated tail, if any, as a line (the capture ended).
    pub fn flush(&mut self) -> Option<String> {
        if self.partial.is_empty() {
            return None;
        }
        let tail = core::mem::take(&mut self.partial);
        let tail = tail.strip_suffix(b"\r").unwrap_or(&tail);
        Some(String::from_utf8_lossy(tail).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn records_read_as_the_logger_printed_them() {
        assert_eq!(
            log_record_lines(b"\x03main: tick"),
            vec!["[INFO] main: tick"]
        );
        assert_eq!(
            log_record_lines(b"\x02a: one\r\ntwo\r\n"),
            vec!["[WARN] a: one", "[WARN] two"]
        );
        assert_eq!(
            log_record_lines(b"\x004 log records dropped"),
            vec!["[LINK] 4 log records dropped"]
        );
        assert!(log_record_lines(b"").is_empty());
    }

    #[test]
    fn text_splits_on_newlines_across_chunks() {
        let mut lines = Vec::new();
        let mut text = TextLines::new();
        text.push(b"[INIT] boot\r\npar", |l| lines.push(l));
        text.push(b"tial\nrest", |l| lines.push(l));
        assert_eq!(lines, vec!["[INIT] boot", "partial"]);
        assert_eq!(text.flush().as_deref(), Some("rest"));
        assert_eq!(text.flush(), None);
    }
}

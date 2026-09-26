//! The playlist's failed-entry warning: its text, written and read in ONE
//! place.
//!
//! A playlist entry whose load or compile fails is marked failed on the
//! device (multi-pattern plan PD9/PD10) and nothing about it crosses the wire
//! as a structured field: the playlist reports it as its runtime status,
//! `Warn("entry 3 failed (load: …); entry 7 failed (compile: …)")`. Studio's
//! Pattern instrument needs the failed KEYS out of that text.
//!
//! So the producer (`lpc-engine`'s playlist node) and the consumer (Studio)
//! both come here: [`format_playlist_failure_status`] writes it and
//! [`parse_playlist_failed_entries`] reads it, and a round-trip test pins the
//! pair so the two cannot drift. Nothing else may assemble or sniff this
//! text.
//!
//! The grammar is `line ("; " line)*` with `line = "entry " key " failed ("
//! reason ")"`. A reason is free text from a compiler or a loader, so the
//! writer replaces any `"; "` inside it with `", "`: the separator then never
//! appears inside a line, and the split is exact whatever the reason says.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// Between two failed-entry lines.
const SEPARATOR: &str = "; ";
/// What a reason's own `"; "` becomes, so it cannot read as a separator.
const SEPARATOR_IN_REASON: &str = ", ";
/// Before the entry key.
const LINE_PREFIX: &str = "entry ";
/// Between the key and the parenthesized reason.
const KEY_SUFFIX: &str = " failed (";
/// After the reason.
const LINE_SUFFIX: &str = ")";

/// The playlist's runtime warning for its failed entries, `None` when no
/// entry has failed.
///
/// `failures` are `(entry key, reason)` pairs in the order the lines should
/// read (the engine passes key order).
#[must_use]
pub fn format_playlist_failure_status<'a>(
    failures: impl IntoIterator<Item = (u32, &'a str)>,
) -> Option<String> {
    let mut status: Option<String> = None;
    for (key, reason) in failures {
        let text = status.get_or_insert_with(String::new);
        if !text.is_empty() {
            text.push_str(SEPARATOR);
        }
        // Writing into a `String` cannot fail.
        let _ = write!(text, "{LINE_PREFIX}{key}{KEY_SUFFIX}");
        text.push_str(&reason.replace(SEPARATOR, SEPARATOR_IN_REASON));
        text.push_str(LINE_SUFFIX);
    }
    status
}

/// The entry keys a playlist's runtime warning names as failed, in the
/// order it names them.
///
/// Text that is not a failure status (another warning, an empty string)
/// names no keys; a line that does not follow the grammar is skipped rather
/// than guessed at.
#[must_use]
pub fn parse_playlist_failed_entries(status: &str) -> Vec<u32> {
    status
        .split(SEPARATOR)
        .filter_map(|line| {
            let rest = line.strip_prefix(LINE_PREFIX)?;
            let (key, rest) = rest.split_once(KEY_SUFFIX)?;
            rest.strip_suffix(LINE_SUFFIX)?;
            key.parse().ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn a_status_round_trips_to_its_keys() {
        let status = format_playlist_failure_status([
            (2, "load: missing file"),
            (7, "compile: expected `;` (line 3)"),
        ])
        .expect("two failures make a status");
        assert_eq!(
            status,
            "entry 2 failed (load: missing file); entry 7 failed (compile: expected `;` (line 3))"
        );
        assert_eq!(parse_playlist_failed_entries(&status), vec![2, 7]);
    }

    /// A reason is free text: a separator inside it must not split a line,
    /// even one that spells out another entry's failure.
    #[test]
    fn a_reason_cannot_forge_a_separator() {
        let status =
            format_playlist_failure_status([(3, "compile: x = 1; entry 9 failed (bogus); y = 2")])
                .expect("status");
        assert_eq!(parse_playlist_failed_entries(&status), vec![3]);
        assert!(
            status.contains("x = 1, entry 9 failed (bogus), y = 2"),
            "the reason reads the same, minus its separators: {status}"
        );
    }

    #[test]
    fn no_failures_is_no_status() {
        assert_eq!(format_playlist_failure_status([]), None);
    }

    #[test]
    fn other_text_names_no_entries() {
        assert!(parse_playlist_failed_entries("").is_empty());
        assert!(parse_playlist_failed_entries("switch refused (pending edits)").is_empty());
        assert!(parse_playlist_failed_entries("entry x failed (why)").is_empty());
        assert!(parse_playlist_failed_entries("entry 4 failed").is_empty());
    }
}

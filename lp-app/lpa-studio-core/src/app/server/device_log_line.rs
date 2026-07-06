//! Pure parser for firmware serial log lines.
//!
//! `fw-core`'s ESP32 logger writes `[{LEVEL}] {module_path}: {message}` (see
//! `lp-fw/fw-core/src/log/esp32.rs`), but the serial stream also carries
//! non-logger output: ESP boot-ROM chatter, panic dumps, and bare prints.
//! This parser splits logger lines into structured parts — level, module
//! path, message remainder — and passes everything else through untouched so
//! raw device output is never mangled.

use crate::UiLogLevel;

/// One device serial line split into its logger parts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceLogLine<'a> {
    /// Severity from the `[LEVEL]` prefix; [`UiLogLevel::Info`] when the line
    /// has none.
    pub level: UiLogLevel,
    /// The logger's module path (e.g. `fw_core::log::esp32`) when the line
    /// matched the logger format. Rides into the UI entry as display-only
    /// source detail.
    pub module: Option<&'a str>,
    /// The message remainder for logger lines; the whole line otherwise.
    pub message: &'a str,
}

/// Parse one serial line from the device.
///
/// - Lines shaped `[LEVEL] module::path: message` yield the level, the module
///   path, and the message remainder. `[TRACE]` maps to
///   [`UiLogLevel::Trace`]. The split is at the *first* `": "` after the
///   module candidate, so messages may contain further colons.
/// - Level-prefixed lines without a plausible module segment (the candidate
///   is empty or contains whitespace, e.g. a bare `println!` that happens to
///   start with `[INFO]`) keep the level but fall back to the whole line as
///   the message — matching the pre-parser prefix-sniffing behavior.
/// - Everything else (boot ROM output, panic dumps, bare prints) falls back
///   to `(Info, no module, whole line)`.
pub fn parse_device_log_line(line: &str) -> DeviceLogLine<'_> {
    let Some((level, rest)) = strip_level_prefix(line) else {
        return DeviceLogLine {
            level: UiLogLevel::Info,
            module: None,
            message: line,
        };
    };
    if let Some(rest) = rest.strip_prefix(' ')
        && let Some((module, message)) = rest.split_once(": ")
        && is_module_path(module)
    {
        return DeviceLogLine {
            level,
            module: Some(module),
            message,
        };
    }
    DeviceLogLine {
        level,
        module: None,
        message: line,
    }
}

/// Strip a leading `[LEVEL]` tag, returning the level and the rest of the
/// line (which normally starts with the separating space).
fn strip_level_prefix(line: &str) -> Option<(UiLogLevel, &str)> {
    const LEVEL_TAGS: [(&str, UiLogLevel); 5] = [
        ("[ERROR]", UiLogLevel::Error),
        ("[WARN]", UiLogLevel::Warn),
        ("[INFO]", UiLogLevel::Info),
        ("[DEBUG]", UiLogLevel::Debug),
        ("[TRACE]", UiLogLevel::Trace),
    ];
    LEVEL_TAGS
        .iter()
        .find_map(|(tag, level)| line.strip_prefix(tag).map(|rest| (*level, rest)))
}

/// Heuristic for "this token is a Rust module path, not message text": module
/// paths are non-empty and never contain whitespace.
fn is_module_path(candidate: &str) -> bool {
    !candidate.is_empty() && !candidate.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_level_prefixes_parse_with_module_and_message() {
        let cases = [
            ("[ERROR] fw_core::a: boom", UiLogLevel::Error),
            ("[WARN] fw_core::a: careful", UiLogLevel::Warn),
            ("[INFO] fw_core::a: hello", UiLogLevel::Info),
            ("[DEBUG] fw_core::a: state", UiLogLevel::Debug),
            ("[TRACE] fw_core::a: tick", UiLogLevel::Trace),
        ];
        for (line, level) in cases {
            let parsed = parse_device_log_line(line);
            assert_eq!(parsed.level, level, "line: {line}");
            assert_eq!(parsed.module, Some("fw_core::a"), "line: {line}");
        }
    }

    #[test]
    fn trace_prefix_maps_to_trace_not_debug() {
        let parsed = parse_device_log_line("[TRACE] lp_engine::frame: rendered");

        assert_eq!(parsed.level, UiLogLevel::Trace);
        assert_eq!(parsed.module, Some("lp_engine::frame"));
        assert_eq!(parsed.message, "rendered");
    }

    #[test]
    fn module_path_is_extracted_and_message_keeps_its_colons() {
        let parsed = parse_device_log_line("[INFO] fw_core::x: took 3: retrying");

        assert_eq!(parsed.level, UiLogLevel::Info);
        assert_eq!(parsed.module, Some("fw_core::x"));
        assert_eq!(parsed.message, "took 3: retrying");
    }

    #[test]
    fn boot_rom_line_falls_back_to_info_whole_line() {
        let line = "ESP-ROM:esp32c6-20220919";
        let parsed = parse_device_log_line(line);

        assert_eq!(parsed.level, UiLogLevel::Info);
        assert_eq!(parsed.module, None);
        assert_eq!(parsed.message, line);
    }

    #[test]
    fn panic_line_falls_back_to_info_whole_line() {
        let line = "!!! PANIC: attempted to subtract with overflow";
        let parsed = parse_device_log_line(line);

        assert_eq!(parsed.level, UiLogLevel::Info);
        assert_eq!(parsed.module, None);
        assert_eq!(parsed.message, line);
    }

    #[test]
    fn bare_text_falls_back_to_info_whole_line() {
        let parsed = parse_device_log_line("hello world");

        assert_eq!(parsed.level, UiLogLevel::Info);
        assert_eq!(parsed.module, None);
        assert_eq!(parsed.message, "hello world");
    }

    #[test]
    fn level_prefix_without_module_keeps_level_and_whole_line() {
        // "free heap" contains a space, so it is message text, not a module
        // path; the level prefix is still honored (pre-parser behavior).
        let line = "[DEBUG] free heap: 12345";
        let parsed = parse_device_log_line(line);

        assert_eq!(parsed.level, UiLogLevel::Debug);
        assert_eq!(parsed.module, None);
        assert_eq!(parsed.message, line);
    }

    #[test]
    fn level_tag_without_separator_keeps_level_and_whole_line() {
        let parsed = parse_device_log_line("[ERROR]");

        assert_eq!(parsed.level, UiLogLevel::Error);
        assert_eq!(parsed.module, None);
        assert_eq!(parsed.message, "[ERROR]");
    }

    #[test]
    fn empty_message_after_module_is_preserved() {
        let parsed = parse_device_log_line("[INFO] fw_core::x: ");

        assert_eq!(parsed.module, Some("fw_core::x"));
        assert_eq!(parsed.message, "");
    }
}

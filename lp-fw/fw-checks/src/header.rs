//! The transcript header a payload prints on the device.
//!
//! One line, `no_std`, no dependency beyond `log` — the same sink
//! `emit_record_json` already uses. It carries only what the firmware knows
//! about itself; everything the host knows (which configuration ran it, the
//! board id, tool versions, what that configuration is trusted for) goes in the
//! `.meta.json` sidecar the runner writes.
//!
//! The two must agree. `lp-emu-validate`'s `Transcript::load` refuses a
//! transcript whose in-band line contradicts its sidecar, and neither may be
//! edited to make them agree — the answer is always a re-capture.
//!
//! The JSON is written by hand rather than through `serde_json` so that a
//! payload with no other reason to pull in `alloc` does not gain one.

use core::fmt::{self, Write as _};

/// The prefix the header line carries.
///
/// Deliberately distinct from [`crate::FW_CHECK_JSON_PREFIX`] so a header is
/// never mistaken for a record by an older parser.
pub const FW_CHECKS_HEADER_PREFIX: &str = "[fw-checks-header] ";

/// The header schema this firmware speaks. Must equal
/// `lp_emu_validate::HEADER_SCHEMA`.
pub const HEADER_SCHEMA: u32 = 1;

/// What the device knows about the image it is.
///
/// Every field comes from a `build.rs`-provided `env!`, so nothing here can
/// drift from the binary it describes:
///
/// ```ignore
/// fw_checks::emit_header(&fw_checks::PayloadHeader {
///     payload: "shader-compile-stress",
///     chip: "esp32c6",
///     firmware_commit: env!("LP_BUILD_COMMIT"),
///     firmware_features: env!("LP_BUILD_FEATURES"),
///     firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
/// });
/// ```
#[derive(Clone, Copy, Debug)]
pub struct PayloadHeader<'a> {
    /// The payload name, as `lp-emu-validate`'s registry knows it.
    pub payload: &'a str,
    pub chip: &'a str,
    /// Short git commit, as `LP_BUILD_COMMIT` reports it.
    pub firmware_commit: &'a str,
    /// The enabled cargo features, comma-separated and sorted, as
    /// `LP_BUILD_FEATURES` reports them.
    pub firmware_features: &'a str,
    pub firmware_dirty: bool,
}

impl fmt::Display for PayloadHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"{{"schema":{HEADER_SCHEMA},"payload":""#)?;
        write_escaped(f, self.payload)?;
        f.write_str(r#"","chip":""#)?;
        write_escaped(f, self.chip)?;
        f.write_str(r#"","firmware_commit":""#)?;
        write_escaped(f, self.firmware_commit)?;
        f.write_str(r#"","firmware_features":""#)?;
        write_escaped(f, self.firmware_features)?;
        write!(f, r#"","firmware_dirty":{}}}"#, self.firmware_dirty)
    }
}

/// Print the header line. Call it once, first, before any record.
///
/// Goes through `log`, so it is a no-op on any harness that installs no
/// logger (`gpio-calibrate`, `uart-bridge`) — that is a real gap on real
/// hardware, not a hypothetical one: a header printed this way from
/// `test_gpio_calibrate` never reached a silicon capture, because that
/// harness's whole point is to stay in `no_std` territory with no `log` sink
/// registered. Kept for host builds and anywhere a logger is known to be
/// installed; a C6 harness should call [`write_header`] instead.
pub fn emit_header(header: &PayloadHeader<'_>) {
    log::info!("{FW_CHECKS_HEADER_PREFIX}{header}");
}

/// Print the header line through any [`fmt::Write`] sink — the
/// sink-agnostic sibling of [`emit_header`], for a harness that cannot rely
/// on a `log` sink being installed (which, from this crate's `no_std` side,
/// is always: installing one is a firmware decision this crate cannot see).
///
/// Every C6 payload harness should call this with `esp_println::Printer`
/// (`uart-bridge`'s reason for going straight to `esp_println` — its printer
/// times out and drops rather than spinning on an unread endpoint — applies
/// equally to every other payload, since none of them can prove a logger is
/// installed either). It writes the same bytes `emit_header` logs, terminated
/// with one `\n`.
///
/// ```ignore
/// let mut printer = esp_println::Printer;
/// let _ = fw_checks::write_header(&mut printer, &fw_checks::PayloadHeader { .. });
/// ```
pub fn write_header(w: &mut impl fmt::Write, header: &PayloadHeader<'_>) -> fmt::Result {
    writeln!(w, "{FW_CHECKS_HEADER_PREFIX}{header}")
}

/// `env!("LP_BUILD_DIRTY")` is the string "true" or "false"; this is the
/// `const`-friendly way to read it without `alloc` or `parse`.
pub const fn str_is_true(s: &str) -> bool {
    matches!(s.as_bytes(), b"true")
}

/// Minimal JSON string escaping: the four characters that can appear in a
/// feature list or a commit and would otherwise produce invalid JSON.
fn write_escaped(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c if (c as u32) < 0x20 => write!(f, "\\u{:04x}", c as u32)?,
            c => f.write_char(c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::format;

    use super::*;

    #[test]
    fn renders_the_contracted_json() {
        let h = PayloadHeader {
            payload: "shader-compile-stress",
            chip: "esp32c6",
            firmware_commit: "d6cfaa2051ae",
            firmware_features: "esp32c6,spike_uart0_link,test_shader_compile_incremental",
            firmware_dirty: false,
        };
        assert_eq!(
            format!("{h}"),
            r#"{"schema":1,"payload":"shader-compile-stress","chip":"esp32c6","firmware_commit":"d6cfaa2051ae","firmware_features":"esp32c6,spike_uart0_link,test_shader_compile_incremental","firmware_dirty":false}"#
        );
    }

    #[test]
    fn escapes_what_would_break_the_json() {
        let h = PayloadHeader {
            payload: "a\"b",
            chip: "c\\d",
            firmware_commit: "e\nf",
            firmware_features: "g",
            firmware_dirty: true,
        };
        let s = format!("{h}");
        assert!(s.contains(r#""payload":"a\"b""#), "{s}");
        assert!(s.contains(r#""chip":"c\\d""#), "{s}");
        assert!(s.contains(r#""firmware_commit":"e\nf""#), "{s}");
        assert!(s.ends_with(r#""firmware_dirty":true}"#), "{s}");
    }

    #[test]
    fn dirty_flag_reads_the_build_env_string() {
        assert!(str_is_true("true"));
        assert!(!str_is_true("false"));
        assert!(!str_is_true(""));
    }

    /// [`write_header`] must print the exact bytes [`emit_header`] would log,
    /// plus one trailing `\n` — that byte-equality is the whole reason a
    /// harness can switch printers without changing what a transcript's
    /// in-band header line looks like.
    #[test]
    fn write_header_matches_the_display_impl_plus_a_newline() {
        let h = PayloadHeader {
            payload: "jit-math-perf",
            chip: "esp32c6",
            firmware_commit: "d6cfaa2051ae",
            firmware_features: "esp32c6,test_jit_math_perf",
            firmware_dirty: false,
        };
        let mut out = std::string::String::new();
        write_header(&mut out, &h).expect("writing to a String never fails");
        assert_eq!(out, format!("{FW_CHECKS_HEADER_PREFIX}{h}\n"));
    }
}

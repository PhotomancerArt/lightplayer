//! Best-effort parse of shader compile-error status text for the editor.
//!
//! Compile failures reach the client as one plain string
//! (`NodeRuntimeStatus::Error`); the engine formats frontend diagnostics
//! rustc-style via `Diagnostic::render` (`lps-glsl/src/diagnostic.rs`):
//!
//! ```text
//! shader compile: error: expected ';', found '}'
//!  --> <shader>:5:18
//!   |
//!  5 | ...
//!   | ^^^
//! ```
//!
//! Parsing is a **client-side presentation concern** — the wire keeps
//! carrying one string, and not every error has a location (recovery-denied
//! "compilation blocked" text, panic messages). Unparseable input degrades
//! to the full text as the message with no location.

/// One shader error prepared for editor display: a headline message, an
/// optional 1-based source location, and the untouched original text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiShaderError {
    /// Headline for the error strip: the first line with the
    /// `shader compile:` / `error:` prefixes stripped.
    pub message: String,
    /// 1-based `(line, col)` from the first ` --> <shader>:L:C` marker.
    pub line_col: Option<(u32, u32)>,
    /// The full original status text (detail/tooltip display).
    pub raw: String,
}

impl UiShaderError {
    /// Parse a `NodeRuntimeStatus::Error` string. Never fails: text without
    /// a recognizable shape becomes a location-less message.
    pub fn parse(status_text: &str) -> Self {
        let first_line = status_text.lines().next().unwrap_or_default();
        let message = strip_error_prefixes(first_line).trim().to_string();
        let message = if message.is_empty() {
            status_text.trim().to_string()
        } else {
            message
        };

        Self {
            message,
            line_col: parse_location_marker(status_text),
            raw: status_text.to_string(),
        }
    }
}

/// Strip the wrapper prefixes down to the human message: the engine's
/// `shader compile:`, generic `Error:`/`error:` wrappers, the `LpsError`
/// stage tag (`parse:`, `lower:`, ...; see `lp-shader/src/error.rs`), and
/// the naga frontend's `GLSL parse error:` (`lps-frontend`'s
/// `CompileError::Parse` display). Observed live wrappings include
/// `shader compile: Error: validation: ...`,
/// `shader compile: parse: error: ...`, and
/// `shader compile: parse: GLSL parse error: error: ...` — the pieces
/// interleave, so peel case-insensitive `error:` around each tag.
fn strip_error_prefixes(line: &str) -> &str {
    const STAGE_PREFIXES: [&str; 5] = ["parse:", "lower:", "compile:", "render:", "validation:"];

    let mut line = line.trim_start();
    line = line
        .strip_prefix("shader compile:")
        .unwrap_or(line)
        .trim_start();
    line = strip_error_word(line);
    for prefix in STAGE_PREFIXES {
        if let Some(rest) = line.strip_prefix(prefix) {
            line = rest.trim_start();
            break;
        }
    }
    line = line
        .strip_prefix("GLSL parse error:")
        .unwrap_or(line)
        .trim_start();
    strip_error_word(line)
}

/// Strip one leading `error:` regardless of case.
fn strip_error_word(line: &str) -> &str {
    match line.get(..6) {
        Some(head) if head.eq_ignore_ascii_case("error:") => line[6..].trim_start(),
        _ => line,
    }
}

/// Find the first location marker at the start of a line: the lps-glsl
/// frontend's rustc-style ` --> <shader>:LINE:COL`, or the naga frontend's
/// codespan-style `┌─ glsl:LINE:COL`. Requiring line-start anchoring keeps
/// a marker fragment inside a string literal on the message line from
/// matching.
fn parse_location_marker(text: &str) -> Option<(u32, u32)> {
    const MARKERS: [&str; 2] = ["--> <shader>:", "┌─ glsl:"];
    for line in text.lines() {
        let head = line.trim_start();
        let Some(rest) = MARKERS.iter().find_map(|m| head.strip_prefix(m)) else {
            continue;
        };
        let mut parts = rest.split(':');
        let line_number = parts.next()?.trim().parse::<u32>().ok()?;
        let col_number = parts
            .next()
            .and_then(|part| part.trim().parse::<u32>().ok())
            .unwrap_or(1);
        if line_number == 0 {
            return None;
        }
        return Some((line_number, col_number));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rendered_diagnostic() {
        let text = "shader compile: error: expected ';', found '}'\n --> <shader>:5:18\n  |\n 5 |     float wave\n  |                ^";
        let parsed = UiShaderError::parse(text);
        assert_eq!(parsed.message, "expected ';', found '}'");
        assert_eq!(parsed.line_col, Some((5, 18)));
        assert_eq!(parsed.raw, text);
    }

    #[test]
    fn strips_the_observed_live_validation_wrapping() {
        // Verbatim from a live simulator session (2026-07-06).
        let parsed =
            UiShaderError::parse("shader compile: Error: validation: no `render` function found");
        assert_eq!(parsed.message, "no `render` function found");
        assert_eq!(parsed.line_col, None);
    }

    #[test]
    fn strips_the_lps_error_stage_tag() {
        // The full production wrapping: ShaderNode's `shader compile:` +
        // `LpsError::Parse`'s `parse:` + the diagnostic's `error:`.
        let text = "shader compile: parse: error: unknown identifier 'wav'\n --> <shader>:6:20";
        let parsed = UiShaderError::parse(text);
        assert_eq!(parsed.message, "unknown identifier 'wav'");
        assert_eq!(parsed.line_col, Some((6, 20)));
    }

    #[test]
    fn parses_the_naga_codespan_diagnostic() {
        // The naga-frontend wrapping: ShaderNode's `shader compile:` +
        // `LpsError::Parse`'s `parse:` + `CompileError::Parse`'s
        // `GLSL parse error:` + naga's codespan render (`error:` headline,
        // `┌─ glsl:L:C` marker).
        let text = "shader compile: parse: GLSL parse error: error: Expected ';', found '}'\n  ┌─ glsl:4:13\n  │\n4 │ float bad = ;\n  │             ^";
        let parsed = UiShaderError::parse(text);
        assert_eq!(parsed.message, "Expected ';', found '}'");
        assert_eq!(parsed.line_col, Some((4, 13)));
    }

    #[test]
    fn recovery_denied_text_has_no_location() {
        let text = "shader compile: compilation blocked after repeated crashes";
        let parsed = UiShaderError::parse(text);
        assert_eq!(parsed.message, "compilation blocked after repeated crashes");
        assert_eq!(parsed.line_col, None);
    }

    #[test]
    fn arbitrary_text_degrades_to_messages_without_location() {
        let parsed = UiShaderError::parse("something unexpected happened");
        assert_eq!(parsed.message, "something unexpected happened");
        assert_eq!(parsed.line_col, None);
    }

    #[test]
    fn marker_inside_message_line_does_not_match() {
        // A `--> <shader>:` fragment inside the message (first line) must
        // not parse as a location; the real marker on its own line must.
        let text = "shader compile: error: bad string \"--> <shader>:9:9\" here\n --> <shader>:2:4";
        let parsed = UiShaderError::parse(text);
        assert_eq!(parsed.line_col, Some((2, 4)));
    }

    #[test]
    fn missing_col_defaults_to_one() {
        let parsed = UiShaderError::parse("error: x\n --> <shader>:7");
        assert_eq!(parsed.line_col, Some((7, 1)));
    }

    #[test]
    fn zero_or_garbage_location_is_dropped() {
        assert_eq!(
            UiShaderError::parse("error: x\n --> <shader>:0:3").line_col,
            None
        );
        assert_eq!(
            UiShaderError::parse("error: x\n --> <shader>:abc:3").line_col,
            None
        );
    }

    #[test]
    fn empty_first_line_falls_back_to_full_text() {
        let parsed = UiShaderError::parse("shader compile: \n --> <shader>:3:1");
        assert_eq!(parsed.message, "shader compile: \n --> <shader>:3:1");
        assert_eq!(parsed.line_col, Some((3, 1)));
    }
}

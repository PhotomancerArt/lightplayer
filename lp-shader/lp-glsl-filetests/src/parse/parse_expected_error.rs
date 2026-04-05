//! Parse inline expected-error directives.
//!
//! Syntax: `// expected-error [E0xxx:] {{message}}` or `// expected-error@+N [E0xxx:] {{message}}`
//! The error code (E0xxx) is optional. Matches compiler output format `error[E0115]: message`.

use crate::parse::test_type::ErrorExpectation;
use anyhow::Result;

const EXPECTED_ERROR: &str = "// expected-error";

/// Parse all error expectations from a line (line_number is 1-indexed).
/// Format: `// expected-error [@+N|@-N] [E0xxx:] {{message}}` where code is optional.
pub fn parse_expected_errors_from_line(
    line: &str,
    line_number: usize,
) -> Result<Vec<ErrorExpectation>> {
    let mut expectations: Vec<ErrorExpectation> = Vec::new();
    let mut remaining = line;

    loop {
        let Some(pos) = remaining.find(EXPECTED_ERROR) else {
            break;
        };
        remaining = &remaining[pos..];

        if !remaining.starts_with(EXPECTED_ERROR) {
            remaining = advance_past_directive(remaining);
            continue;
        }

        let after_prefix = remaining.strip_prefix(EXPECTED_ERROR).unwrap_or(remaining);
        let (offset, after_offset) = parse_line_offset(after_prefix);
        let effective_line = ((line_number as i32) + offset).max(1) as usize;

        let (code, after_code) = parse_optional_code(after_offset);
        let msg = extract_brace_content(after_code);

        let exp = ErrorExpectation {
            line: effective_line,
            message: msg,
            code,
        };
        expectations.push(exp);

        remaining = advance_past_directive(remaining);
    }

    Ok(expectations)
}

/// Parse optional `E0xxx:` prefix. Returns (Some(code), rest) or (None, s) if no code.
fn parse_optional_code(s: &str) -> (Option<String>, &str) {
    let s = s.trim_start();
    if s.starts_with('E') {
        let rest = s.strip_prefix('E').unwrap_or(s);
        let digit_end = rest
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_digit())
            .last()
            .map(|(i, _)| i + 1)
            .unwrap_or(0);
        if digit_end > 0 {
            let code = format!("E{}", &rest[..digit_end]);
            let rest = &rest[digit_end..];
            if rest.trim_start().starts_with(':') {
                let after_colon = rest
                    .trim_start()
                    .strip_prefix(':')
                    .unwrap_or(rest)
                    .trim_start();
                return (Some(code), after_colon);
            }
        }
    }
    (None, s)
}

/// Extract content between `{{` and `}}`.
fn extract_brace_content(s: &str) -> Option<String> {
    let open = s.find("{{")?;
    let rest = &s[open + 2..];
    let close = rest.find("}}")?;
    Some(rest[..close].trim().to_string())
}

/// Parse @+N or @-N offset. Returns (offset, rest_after_offset).
fn parse_line_offset(s: &str) -> (i32, &str) {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('@') {
        let (sign, num_part) = if let Some(p) = rest.strip_prefix('+') {
            (1i32, p)
        } else if let Some(p) = rest.strip_prefix('-') {
            (-1i32, p)
        } else {
            return (0, s);
        };
        let digit_len = num_part.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digit_len > 0 {
            if let Ok(n) = num_part[..digit_len].parse::<i32>() {
                let after = num_part[digit_len..].trim_start();
                return (sign * n, after);
            }
        }
    }
    (0, s)
}

/// Advance past the current directive (to next // or end of string).
fn advance_past_directive(s: &str) -> &str {
    if let Some(pos) = s[1..].find("//") {
        &s[1 + pos..]
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expected_error_basic() {
        let line = "    5++;  // expected-error {{increment/decrement only supported}}";
        let result = parse_expected_errors_from_line(line, 6).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6);
        assert_eq!(
            result[0].message,
            Some("increment/decrement only supported".to_string())
        );
        assert_eq!(result[0].code, None);
    }

    #[test]
    fn test_parse_expected_error_with_code() {
        let line = "    5++;  // expected-error E0115: {{expression is not a valid LValue}}";
        let result = parse_expected_errors_from_line(line, 6).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6);
        assert_eq!(
            result[0].message,
            Some("expression is not a valid LValue".to_string())
        );
        assert_eq!(result[0].code, Some("E0115".to_string()));
    }

    #[test]
    fn test_parse_expected_error_at_plus_one() {
        let line = "    int x =  // expected-error@+1 {{expected expression}}";
        let result = parse_expected_errors_from_line(line, 4).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
        assert_eq!(result[0].message, Some("expected expression".to_string()));
    }

    #[test]
    fn test_parse_expected_error_at_minus_one() {
        let line = "    ;  // expected-error@-1 {{expected expression}}";
        let result = parse_expected_errors_from_line(line, 5).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_parse_expected_error_none() {
        let result = parse_expected_errors_from_line("    int x = 5;", 3).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_expected_error_with_code_and_offset() {
        let line = "    int x =  // expected-error@+1 E0115: {{some error}}";
        let result = parse_expected_errors_from_line(line, 4).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
        assert_eq!(result[0].message, Some("some error".to_string()));
        assert_eq!(result[0].code, Some("E0115".to_string()));
    }
}

//! Parse trap expectations.

use crate::parse::test_type::TrapExpectation;
use anyhow::Result;

/// Parse trap expectation from a line.
/// Supports `// EXPECT_TRAP: <message>` or `// EXPECT_TRAP_CODE: <code>`
pub fn parse_trap_expectation(line: &str, line_number: usize) -> Result<Option<TrapExpectation>> {
    let trimmed = line.trim();

    if let Some(message) = trimmed.strip_prefix("// EXPECT_TRAP:") {
        Ok(Some(TrapExpectation {
            trap_code: None,
            trap_message: Some(message.trim().to_string()),
            line_number,
        }))
    } else if let Some(code_str) = trimmed.strip_prefix("// EXPECT_TRAP_CODE:") {
        let code = code_str
            .trim()
            .parse::<u8>()
            .map_err(|e| anyhow::anyhow!("invalid trap code at line {line_number}: {e}"))?;
        Ok(Some(TrapExpectation {
            trap_code: Some(code),
            trap_message: None,
            line_number,
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trap_expectation_message() {
        let result = parse_trap_expectation("// EXPECT_TRAP: division by zero", 1).unwrap();
        assert!(result.is_some());
        let trap = result.unwrap();
        assert_eq!(trap.trap_code, None);
        assert_eq!(trap.trap_message, Some("division by zero".to_string()));
        assert_eq!(trap.line_number, 1);
    }

    #[test]
    fn test_parse_trap_expectation_code() {
        let result = parse_trap_expectation("// EXPECT_TRAP_CODE: 42", 2).unwrap();
        assert!(result.is_some());
        let trap = result.unwrap();
        assert_eq!(trap.trap_code, Some(42));
        assert_eq!(trap.trap_message, None);
        assert_eq!(trap.line_number, 2);
    }

    #[test]
    fn test_parse_trap_expectation_with_whitespace() {
        let result = parse_trap_expectation("  // EXPECT_TRAP: message  ", 3).unwrap();
        assert!(result.is_some());
        let trap = result.unwrap();
        assert_eq!(trap.trap_message, Some("message".to_string()));
    }

    #[test]
    fn test_parse_trap_expectation_none() {
        assert_eq!(parse_trap_expectation("// not a trap", 4).unwrap(), None);
        assert_eq!(
            parse_trap_expectation("EXPECT_TRAP: message", 5).unwrap(),
            None
        );
        assert_eq!(parse_trap_expectation("", 6).unwrap(), None);
    }

    #[test]
    fn test_parse_trap_expectation_invalid_code() {
        assert!(parse_trap_expectation("// EXPECT_TRAP_CODE: 999", 7).is_err());
        assert!(parse_trap_expectation("// EXPECT_TRAP_CODE: abc", 8).is_err());
    }
}

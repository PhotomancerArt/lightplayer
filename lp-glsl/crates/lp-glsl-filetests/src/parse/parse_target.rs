//! Parse target directive.

/// Parse target directive from a line.
pub fn parse_target_directive(line: &str) -> Option<String> {
    line.trim()
        .strip_prefix("// target ")
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_directive() {
        assert_eq!(
            parse_target_directive("// target riscv32.q32"),
            Some("riscv32.q32".to_string())
        );
        assert_eq!(
            parse_target_directive("  // target riscv32.q32  "),
            Some("riscv32.q32".to_string())
        );
        assert_eq!(
            parse_target_directive("// target riscv32.float"),
            Some("riscv32.float".to_string())
        );
    }

    #[test]
    fn test_parse_target_directive_invalid() {
        assert_eq!(parse_target_directive("// target"), None);
        assert_eq!(parse_target_directive("target riscv32.q32"), None);
        assert_eq!(parse_target_directive("// target "), None);
        assert_eq!(parse_target_directive(""), None);
    }
}

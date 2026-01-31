//! Parse test type directives.

use crate::parse::test_type::TestType;

/// Parse test type from a line.
pub fn parse_test_type(line: &str) -> Option<TestType> {
    let trimmed = line.trim();
    if trimmed == "// test compile" {
        Some(TestType::Compile)
    } else if trimmed == "// test transform.q32" {
        Some(TestType::TransformQ32)
    } else if trimmed.starts_with("// test run") {
        Some(TestType::Run)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_test_type_compile() {
        assert_eq!(parse_test_type("// test compile"), Some(TestType::Compile));
        assert_eq!(
            parse_test_type("  // test compile  "),
            Some(TestType::Compile)
        );
    }

    #[test]
    fn test_parse_test_type_transform() {
        assert_eq!(
            parse_test_type("// test transform.q32"),
            Some(TestType::TransformQ32)
        );
        assert_eq!(
            parse_test_type("  // test transform.q32  "),
            Some(TestType::TransformQ32)
        );
    }

    #[test]
    fn test_parse_test_type_run() {
        assert_eq!(parse_test_type("// test run"), Some(TestType::Run));
        assert_eq!(parse_test_type("// test run "), Some(TestType::Run));
        assert_eq!(parse_test_type("  // test run  "), Some(TestType::Run));
    }

    #[test]
    fn test_parse_test_type_invalid() {
        assert_eq!(parse_test_type("// test"), None);
        assert_eq!(parse_test_type("test run"), None);
        assert_eq!(parse_test_type("// test compile extra"), None);
        assert_eq!(parse_test_type(""), None);
    }
}

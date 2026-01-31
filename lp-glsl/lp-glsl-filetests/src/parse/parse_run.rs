//! Parse run directives.

use crate::parse::test_type::{ComparisonOp, RunDirective};
use anyhow::Result;

/// Parse run directive from a line.
pub fn parse_run_directive_line(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("// run:")
        .or_else(|| trimmed.strip_prefix("// #run:"))
        .map(|s| s.trim())
}

/// Parse a single `// run:` line into a `RunDirective`.
pub fn parse_run_directive(line: &str, line_number: usize) -> Result<RunDirective> {
    // Check for [expect-fail] marker at the end and strip it first
    let (line_without_marker, expect_fail) = if line.trim_end().ends_with("[expect-fail]") {
        let without_marker = line
            .trim_end()
            .strip_suffix("[expect-fail]")
            .unwrap()
            .trim_end();
        (without_marker, true)
    } else {
        (line, false)
    };

    // Parse format: <expression> == <expected> or <expression> ~= <expected> [ (tolerance: <value>) ]
    let (comparison, expr, expected_with_tolerance) = if let Some(pos) =
        line_without_marker.rfind(" == ")
    {
        let expr = line_without_marker[..pos].trim();
        let expected = line_without_marker[pos + 4..].trim();
        (ComparisonOp::Exact, expr, expected)
    } else if let Some(pos) = line_without_marker.rfind(" ~= ") {
        let expr = line_without_marker[..pos].trim();
        let expected = line_without_marker[pos + 4..].trim();
        (ComparisonOp::Approx, expr, expected)
    } else {
        anyhow::bail!("invalid run directive format at line {line_number}: expected '==' or '~='");
    };

    // Strip comments from expected value (comments start with //)
    let expected_with_tolerance = if let Some(comment_pos) = expected_with_tolerance.find("//") {
        expected_with_tolerance[..comment_pos].trim()
    } else {
        expected_with_tolerance
    };

    // Parse tolerance if present: (tolerance: 0.001)
    let (expected, tolerance) =
        if let Some(tolerance_start) = expected_with_tolerance.find("(tolerance:") {
            let expected = expected_with_tolerance[..tolerance_start].trim();
            let tolerance_str = expected_with_tolerance[tolerance_start..]
                .strip_prefix("(tolerance:")
                .and_then(|s| s.strip_suffix(")"))
                .map(|s| s.trim());

            let tolerance = if let Some(tol_str) = tolerance_str {
                Some(tol_str.parse::<f32>().map_err(|e| {
                    anyhow::anyhow!("invalid tolerance value at line {line_number}: {e}")
                })?)
            } else {
                None
            };

            (expected, tolerance)
        } else {
            (expected_with_tolerance, None)
        };

    Ok(RunDirective {
        expression_str: expr.to_string(),
        comparison,
        expected_str: expected.to_string(),
        tolerance,
        line_number,
        expect_fail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::test_type::ComparisonOp;

    #[test]
    fn test_parse_run_directive_line() {
        assert_eq!(
            parse_run_directive_line("// run: add_int(0, 0) == 0"),
            Some("add_int(0, 0) == 0")
        );
        assert_eq!(
            parse_run_directive_line("  // run: test() == 1  "),
            Some("test() == 1")
        );
        assert_eq!(
            parse_run_directive_line("// #run: test() == 1"),
            Some("test() == 1")
        );
        assert_eq!(parse_run_directive_line("// run:"), Some(""));
        assert_eq!(parse_run_directive_line("not a run directive"), None);
    }

    #[test]
    fn test_parse_run_directive_exact() {
        let dir = parse_run_directive("add_int(0, 0) == 0", 1).unwrap();
        assert_eq!(dir.expression_str, "add_int(0, 0)");
        assert_eq!(dir.comparison, ComparisonOp::Exact);
        assert_eq!(dir.expected_str, "0");
        assert_eq!(dir.tolerance, None);
        assert_eq!(dir.line_number, 1);
    }

    #[test]
    fn test_parse_run_directive_approx() {
        let dir = parse_run_directive("add_float(1.5, 2.5) ~= 4.0", 2).unwrap();
        assert_eq!(dir.expression_str, "add_float(1.5, 2.5)");
        assert_eq!(dir.comparison, ComparisonOp::Approx);
        assert_eq!(dir.expected_str, "4.0");
        assert_eq!(dir.tolerance, None);
        assert_eq!(dir.line_number, 2);
    }

    #[test]
    fn test_parse_run_directive_with_tolerance() {
        let dir = parse_run_directive("test() ~= 1.0 (tolerance: 0.001)", 3).unwrap();
        assert_eq!(dir.expression_str, "test()");
        assert_eq!(dir.comparison, ComparisonOp::Approx);
        assert_eq!(dir.expected_str, "1.0");
        assert_eq!(dir.tolerance, Some(0.001));
        assert_eq!(dir.line_number, 3);
    }

    #[test]
    fn test_parse_run_directive_with_comment() {
        let dir = parse_run_directive("test() == 1 // comment", 4).unwrap();
        assert_eq!(dir.expression_str, "test()");
        assert_eq!(dir.comparison, ComparisonOp::Exact);
        assert_eq!(dir.expected_str, "1");
        assert_eq!(dir.line_number, 4);
    }

    #[test]
    fn test_parse_run_directive_invalid() {
        assert!(parse_run_directive("test()", 1).is_err());
        assert!(parse_run_directive("test() = 1", 1).is_err());
        assert!(parse_run_directive("", 1).is_err());
    }

    #[test]
    fn test_parse_run_directive_with_expect_fail() {
        let dir = parse_run_directive("test() == 1 [expect-fail]", 5).unwrap();
        assert_eq!(dir.expression_str, "test()");
        assert_eq!(dir.comparison, ComparisonOp::Exact);
        assert_eq!(dir.expected_str, "1");
        assert_eq!(dir.expect_fail, true);
        assert_eq!(dir.line_number, 5);
    }

    #[test]
    fn test_parse_run_directive_with_expect_fail_and_tolerance() {
        let dir = parse_run_directive("test() ~= 1.0 (tolerance: 0.001) [expect-fail]", 6).unwrap();
        assert_eq!(dir.expression_str, "test()");
        assert_eq!(dir.comparison, ComparisonOp::Approx);
        assert_eq!(dir.expected_str, "1.0");
        assert_eq!(dir.tolerance, Some(0.001));
        assert_eq!(dir.expect_fail, true);
        assert_eq!(dir.line_number, 6);
    }

    #[test]
    fn test_parse_run_directive_with_expect_fail_and_comment() {
        let dir = parse_run_directive("test() == 1 // comment [expect-fail]", 7).unwrap();
        assert_eq!(dir.expression_str, "test()");
        assert_eq!(dir.comparison, ComparisonOp::Exact);
        assert_eq!(dir.expected_str, "1");
        assert_eq!(dir.expect_fail, true);
        assert_eq!(dir.line_number, 7);
    }

    #[test]
    fn test_parse_run_directive_without_expect_fail() {
        let dir = parse_run_directive("test() == 1", 8).unwrap();
        assert_eq!(dir.expect_fail, false);
    }
}

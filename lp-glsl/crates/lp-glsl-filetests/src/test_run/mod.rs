//! Run test implementation.

pub mod execution;
pub mod parse_assert;
pub mod run;
pub mod run_detail;
pub mod run_summary;
pub mod target;
pub mod test_glsl;

// Re-exports
pub use run::{run_test_file, run_test_file_with_line_filter};

/// Statistics for test case execution within a file.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestCaseStats {
    /// Number of test cases that passed (excluding expect-fail tests).
    pub passed: usize,
    /// Number of test cases that failed unexpectedly (regressions).
    pub failed: usize,
    /// Total number of test cases.
    pub total: usize,
    /// Number of tests marked `[expect-fail]` that failed (as expected).
    pub expect_fail: usize,
    /// Number of tests marked `[expect-fail]` that passed (unexpected pass).
    pub unexpected_pass: usize,
}

/// Helper function to record a test failure, respecting [expect-fail] markers.
/// If the directive is marked as expect_fail, counts it as an expected failure.
/// Otherwise, counts it as an unexpected failure and adds the line number to failed_lines.
pub fn record_failure(
    directive: &crate::parse::test_type::RunDirective,
    stats: &mut TestCaseStats,
    failed_lines: &mut Vec<usize>,
) {
    if directive.expect_fail {
        stats.expect_fail += 1;
    } else {
        stats.failed += 1;
        failed_lines.push(directive.line_number);
    }
}

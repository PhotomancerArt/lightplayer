//! Run test implementation.

pub mod compile;
pub mod execution;
pub mod parse_assert;
pub mod run;
pub mod run_detail;
pub mod run_summary;
pub mod test_glsl;
pub mod wasm_runner;

// Re-exports
pub use run::{run_test_file, run_test_file_with_line_filter};

use crate::target::Disposition;

/// Statistics for test case execution within a file.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestCaseStats {
    /// Number of test cases that passed.
    pub passed: usize,
    /// Number of test cases that failed unexpectedly (regressions).
    pub failed: usize,
    /// Total number of test cases.
    pub total: usize,
    /// Tests annotated @unimplemented/@broken that failed as expected.
    pub expected_failure: usize,
    /// Tests annotated @unimplemented/@broken that unexpectedly passed.
    pub unexpected_pass: usize,
    /// Tests annotated @ignore that were skipped.
    pub skipped: usize,
}

/// Record a test result based on disposition and pass/fail.
pub fn record_result(
    disposition: Disposition,
    passed: bool,
    stats: &mut TestCaseStats,
    failed_lines: &mut Vec<usize>,
    unexpected_pass_lines: &mut Vec<usize>,
    line_number: usize,
) {
    match (disposition, passed) {
        (Disposition::Skip, _) => {
            stats.skipped += 1;
        }
        (Disposition::ExpectFailure, true) => {
            stats.unexpected_pass += 1;
            unexpected_pass_lines.push(line_number);
        }
        (Disposition::ExpectFailure, false) => {
            stats.expected_failure += 1;
        }
        (Disposition::ExpectSuccess, true) => {
            stats.passed += 1;
        }
        (Disposition::ExpectSuccess, false) => {
            stats.failed += 1;
            failed_lines.push(line_number);
        }
    }
}

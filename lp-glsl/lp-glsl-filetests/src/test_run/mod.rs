//! Run test implementation.

pub mod compile;
pub mod execution;
pub mod lpir_jit_executable;
pub mod lpir_rv32_executable;
pub mod parse_assert;
pub mod q32_exec_common;
pub mod run;
pub mod run_detail;
pub mod run_summary;
pub mod test_glsl;
pub mod wasm_link;
pub mod wasm_runner;

// Re-exports
pub use run::{PerTargetStats, run_test_file, run_test_file_with_line_filter};

use crate::target::{AnnotationKind, Disposition};

/// Statistics for test case execution within a file.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestCaseStats {
    /// Number of test cases that passed.
    pub passed: usize,
    /// Number of test cases that failed unexpectedly (regressions).
    pub failed: usize,
    /// Total number of test cases.
    pub total: usize,
    /// Tests annotated @unimplemented that failed as expected.
    pub unimplemented: usize,
    /// Tests annotated @broken that failed as expected.
    pub broken: usize,
    /// Tests annotated @unimplemented/@broken that unexpectedly passed.
    pub unexpected_pass: usize,
    /// Tests annotated @ignore that were skipped.
    pub skipped: usize,
}

impl TestCaseStats {
    /// Total expected-failure count (unimplemented + broken).
    pub fn expected_failure(&self) -> usize {
        self.unimplemented + self.broken
    }

    /// Add another stats into this one.
    pub fn add(&mut self, other: impl std::borrow::Borrow<TestCaseStats>) {
        let o = other.borrow();
        self.passed += o.passed;
        self.failed += o.failed;
        self.total += o.total;
        self.unimplemented += o.unimplemented;
        self.broken += o.broken;
        self.unexpected_pass += o.unexpected_pass;
        self.skipped += o.skipped;
    }
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
    match (&disposition, passed) {
        (Disposition::Skip, _) => {
            stats.skipped += 1;
        }
        (Disposition::ExpectFailure(_), true) => {
            stats.unexpected_pass += 1;
            unexpected_pass_lines.push(line_number);
        }
        (Disposition::ExpectFailure(AnnotationKind::Unimplemented), false) => {
            stats.unimplemented += 1;
        }
        (Disposition::ExpectFailure(AnnotationKind::Broken | AnnotationKind::Ignore), false) => {
            stats.broken += 1;
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

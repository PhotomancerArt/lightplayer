//! Run test implementation.

pub mod compile;
pub mod execution;
pub mod filetest_lpvm;
pub mod parse_assert;
pub mod run;
pub mod run_detail;
pub mod set_uniform;

// Re-exports
pub use run::{PerTargetStats, run_test_file, run_test_file_with_line_filter};

use crate::targets::{AnnotationKind, Disposition};

/// Statistics for test case execution within a file.
#[derive(Debug, Clone, Default)]
pub struct TestCaseStats {
    /// Number of test cases that passed.
    pub passed: usize,
    /// Number of test cases that failed unexpectedly (regressions).
    pub failed: usize,
    /// Total number of test cases.
    pub total: usize,
    /// Tests annotated @unimplemented that failed as expected.
    pub unimplemented: usize,
    /// Tests annotated @unimplemented that unexpectedly passed.
    pub unexpected_pass: usize,
    /// Tests annotated @unsupported for this target (skipped — not applicable by design).
    pub unsupported: usize,
    /// Sum of guest RV32 instructions for successful `// run:` executions (emu backends only).
    pub guest_instructions_total: u64,
    /// Sum of guest cycle estimates (same runs as [`Self::guest_instructions_total`]).
    pub guest_cycles_total: u64,
}

impl TestCaseStats {
    /// Total expected-failure count (unimplemented only; unsupported is separate).
    pub fn expected_failure(&self) -> usize {
        self.unimplemented
    }

    /// Add another stats into this one.
    pub fn add(&mut self, other: impl std::borrow::Borrow<TestCaseStats>) {
        let o = other.borrow();
        self.passed += o.passed;
        self.failed += o.failed;
        self.total += o.total;
        self.unimplemented += o.unimplemented;
        self.unexpected_pass += o.unexpected_pass;
        self.unsupported += o.unsupported;
        self.guest_instructions_total += o.guest_instructions_total;
        self.guest_cycles_total += o.guest_cycles_total;
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
            stats.unsupported += 1;
        }
        (Disposition::ExpectFailure(AnnotationKind::Unsupported), _) => {
            // Defensive: Unsupported normally maps to Skip in directive_disposition.
            stats.unsupported += 1;
        }
        (Disposition::ExpectFailure(_), true) => {
            stats.unexpected_pass += 1;
            unexpected_pass_lines.push(line_number);
        }
        (Disposition::ExpectFailure(AnnotationKind::Unimplemented), false)
        | (Disposition::ExpectFailure(AnnotationKind::Broken), false) => {
            stats.unimplemented += 1;
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

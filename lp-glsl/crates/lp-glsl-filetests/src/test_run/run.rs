//! Main delegator for run tests (chooses summary vs detail).

use crate::output_mode::OutputMode;
use crate::parse::TestFile;
use crate::test_run::{TestCaseStats, run_detail, run_summary};
use anyhow::Result;
use std::path::Path;

/// Run all tests in a test file with optional line number filtering.
/// Returns the result, test case statistics, line numbers with unexpected passes, and line numbers that failed.
pub fn run_test_file_with_line_filter(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)> {
    if !test_file.is_test_run {
        // Not a test run file, skip
        return Ok((Ok(()), TestCaseStats::default(), Vec::new(), Vec::new()));
    }

    match output_mode {
        OutputMode::Summary => run_summary::run(test_file, path, line_filter),
        OutputMode::Detail | OutputMode::Debug => {
            run_detail::run(test_file, path, line_filter, output_mode)
        }
    }
}

/// Run all tests in a test file.
pub fn run_test_file(test_file: &TestFile, path: &Path) -> Result<()> {
    let (result, _stats, _, _) = run_test_file_with_line_filter(
        test_file,
        path,
        None,
        OutputMode::Detail, // Default to detail mode for single file
    )?;
    result
}

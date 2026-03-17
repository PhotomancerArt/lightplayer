//! Main delegator for run tests (chooses summary vs detail).

use crate::output_mode::OutputMode;
use crate::parse::TestFile;
use crate::target::Target;
use crate::test_run::{TestCaseStats, run_detail, run_summary};
use anyhow::Result;
use std::path::Path;

/// Run all tests in a test file with optional line number filtering.
/// Iterates over the given targets; if the file has @ignore for all targets, skips.
/// Returns the combined result, aggregated stats, unexpected pass lines, and failed lines.
pub fn run_test_file_with_line_filter(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    targets: &[&Target],
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)> {
    let is_test_run = test_file.test_types.contains(&crate::parse::TestType::Run);
    if !is_test_run {
        return Ok((Ok(()), TestCaseStats::default(), Vec::new(), Vec::new()));
    }

    if targets.is_empty() {
        return Ok((Ok(()), TestCaseStats::default(), Vec::new(), Vec::new()));
    }

    let mut combined_stats = TestCaseStats::default();
    let mut all_unexpected_pass = Vec::new();
    let mut all_failed_lines = Vec::new();
    let mut overall_result = Ok(());

    for target in targets {
        let (result, stats, unexpected_pass, failed_lines) = match output_mode {
            OutputMode::Summary => run_summary::run(test_file, path, line_filter, target)?,
            OutputMode::Detail | OutputMode::Debug => {
                run_detail::run(test_file, path, line_filter, output_mode, target)?
            }
        };

        combined_stats.passed += stats.passed;
        combined_stats.failed += stats.failed;
        combined_stats.total += stats.total;
        combined_stats.expected_failure += stats.expected_failure;
        combined_stats.unexpected_pass += stats.unexpected_pass;
        combined_stats.skipped += stats.skipped;

        all_unexpected_pass.extend(unexpected_pass);
        all_failed_lines.extend(failed_lines);

        if overall_result.is_ok() && result.is_err() {
            overall_result = result;
        }
    }

    Ok((
        overall_result,
        combined_stats,
        all_unexpected_pass,
        all_failed_lines,
    ))
}

/// Run all tests in a test file (single target for backward compat).
pub fn run_test_file(test_file: &TestFile, path: &Path) -> Result<()> {
    let targets: Vec<&Target> = crate::target::DEFAULT_TARGETS.iter().collect();
    let (result, _stats, _, _) =
        run_test_file_with_line_filter(test_file, path, None, OutputMode::Detail, &targets)?;
    result
}

//! Main delegator for run tests.

use crate::output_mode::OutputMode;
use crate::parse::TestFile;
use crate::targets::Target;
use crate::test_run::{TestCaseStats, run_detail};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// Per-target stats for summary table. Key = target name (e.g. "jit.q32").
pub type PerTargetStats = BTreeMap<String, TestCaseStats>;

/// Run all tests in a test file with optional line number filtering.
/// Iterates over the given targets; if the file has @unsupported for all targets, skips.
/// Returns the combined result, per-target stats, aggregated stats, unexpected-pass lines per
/// target, failed lines per target, compile-failed per target, and whether any target had a
/// whole-file compile failure.
pub fn run_test_file_with_line_filter(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    targets: &[&Target],
    suppress_rerun: bool,
) -> Result<(
    Result<()>,
    PerTargetStats,
    TestCaseStats,
    BTreeMap<String, Vec<usize>>,
    BTreeMap<String, Vec<usize>>,
    BTreeMap<String, bool>,
    bool,
)> {
    let is_test_run = test_file.test_types.contains(&crate::parse::TestType::Run);
    if !is_test_run {
        return Ok((
            Ok(()),
            BTreeMap::new(),
            TestCaseStats::default(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            false,
        ));
    }

    if targets.is_empty() {
        return Ok((
            Ok(()),
            BTreeMap::new(),
            TestCaseStats::default(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            false,
        ));
    }

    let mut combined_stats = TestCaseStats::default();
    let mut per_target = BTreeMap::new();
    let mut unexpected_pass_by_target: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut failed_lines_by_target: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut compile_failed_by_target: BTreeMap<String, bool> = BTreeMap::new();
    let mut overall_result = Ok(());

    for target in targets {
        let (result, stats, unexpected_pass, failed_lines, compile_failed) =
            run_detail::run(test_file, path, line_filter, output_mode, target, suppress_rerun)?;

        let target_name = target.name();
        compile_failed_by_target.insert(target_name.clone(), compile_failed);

        combined_stats.passed += stats.passed;
        combined_stats.failed += stats.failed;
        combined_stats.total += stats.total;
        combined_stats.unimplemented += stats.unimplemented;
        combined_stats.unexpected_pass += stats.unexpected_pass;
        combined_stats.unsupported += stats.unsupported;
        if targets.len() == 1 {
            combined_stats.guest_instructions_total = stats.guest_instructions_total;
        }
        per_target.insert(target_name.clone(), stats);

        if !unexpected_pass.is_empty() {
            unexpected_pass_by_target.insert(target_name.clone(), unexpected_pass);
        }
        if !failed_lines.is_empty() {
            failed_lines_by_target.insert(target_name.clone(), failed_lines);
        }

        if overall_result.is_ok() && result.is_err() {
            overall_result = result;
        }
    }

    let any_compile_failed = compile_failed_by_target.values().any(|&cf| cf);

    Ok((
        overall_result,
        per_target,
        combined_stats,
        unexpected_pass_by_target,
        failed_lines_by_target,
        compile_failed_by_target,
        any_compile_failed,
    ))
}

/// Run all tests in a test file (single target for backward compat).
pub fn run_test_file(test_file: &TestFile, path: &Path) -> Result<()> {
    let targets: Vec<&Target> = crate::targets::DEFAULT_TARGETS.iter().collect();
    let (result, _, _, _, _, _, _) =
        run_test_file_with_line_filter(test_file, path, None, OutputMode::Detail, &targets, false)?;
    result
}

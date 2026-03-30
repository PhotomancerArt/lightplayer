//! GLSL filetests infrastructure.
//!
//! This crate provides infrastructure for discovering, parsing, compiling, executing, and
//! verifying GLSL test files, matching Cranelift's filetests semantics.

#![deny(missing_docs)]

pub mod colors;
pub mod discovery;
pub mod output_mode;
pub mod parse;
pub mod runner;
pub mod target;
pub mod test_compile;
pub mod test_error;
pub mod test_run;
pub mod test_transform;
pub mod util;

use anyhow::Result;
use glob::{MatchOptions, glob_with};
use output_mode::OutputMode;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

use crate::target::{Backend, DEFAULT_TARGETS, Target};

/// Adds `@unimplemented(backend=…)` for one run-test file: file-level when the whole module
/// failed to compile in summary mode; otherwise before each failing `// run:`. Returns how many
/// marker operations were applied (0 if already annotated).
fn mark_unimplemented_expectations_for_file(
    path: &Path,
    failed_lines: &[usize],
    compile_failed: bool,
    use_file_level_for_compile_fail: bool,
    target: &Target,
) -> anyhow::Result<usize> {
    let ann = format!(
        "// @unimplemented(backend={})",
        match target.backend {
            Backend::Jit => "jit",
            Backend::Rv32 => "rv32",
            Backend::Wasm => "wasm",
        }
    );
    let mut n = 0;
    if compile_failed && use_file_level_for_compile_fail {
        let u = util::file_update::FileUpdate::new(path);
        if u.ensure_file_level_unimplemented(target)? {
            n += 1;
        }
        return Ok(n);
    }

    let u = util::file_update::FileUpdate::new(path);
    let mut sorted: Vec<usize> = failed_lines.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for line in sorted {
        if u.per_directive_unimplemented_present(line, target)? {
            continue;
        }
        u.add_annotation(line, &ann)?;
        n += 1;
    }
    Ok(n)
}

/// Run a single filetest.
pub fn run_filetest(path: &Path) -> Result<()> {
    let targets: Vec<&Target> = DEFAULT_TARGETS.iter().collect();
    let (result, _, _, _, _, _) =
        run_filetest_with_line_filter(path, None, OutputMode::Detail, &targets)?;
    result
}

/// Count test cases in a file by counting `// run:` directives.
/// This works even if parsing fails later, so we can show stats.
pub(crate) fn count_test_cases(path: &Path, line_filter: Option<usize>) -> test_run::TestCaseStats {
    let mut stats = test_run::TestCaseStats::default();

    // Try to read and count run directives
    if let Ok(contents) = std::fs::read_to_string(path) {
        for (line_num, line) in contents.lines().enumerate() {
            let line_number = line_num + 1;

            // Apply line filter if provided
            if let Some(filter_line) = line_filter {
                if line_number != filter_line {
                    continue;
                }
            }

            // Check if this line contains a run directive
            if parse::parse_run::parse_run_directive_line(line).is_some() {
                stats.total += 1;
            }
        }
    }

    stats
}

/// Run a single filetest with optional line number filtering.
/// Returns the result, per-target stats, combined stats, unexpected-pass lines, failed lines, and
/// whether any target had a whole-file compile failure (summary mode).
pub fn run_filetest_with_line_filter(
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    targets: &[&Target],
) -> Result<(
    Result<()>,
    test_run::PerTargetStats,
    test_run::TestCaseStats,
    Vec<usize>,
    Vec<usize>,
    bool,
)> {
    // Count test cases early, even if parsing fails later
    let early_stats = count_test_cases(path, line_filter);

    let test_file = match parse::parse_test_file(path) {
        Ok(tf) => tf,
        Err(e) => {
            // Return error but preserve the test case count we already computed
            return Ok((
                Err(e),
                BTreeMap::new(),
                early_stats,
                Vec::new(),
                Vec::new(),
                false,
            ));
        }
    };

    // Validate line number if provided (only for run tests; error tests ignore line filter)
    if let Some(line_number) = line_filter {
        if !test_file.test_types.contains(&parse::TestType::Error) {
            let has_matching_directive = test_file
                .run_directives
                .iter()
                .any(|directive| directive.line_number == line_number);
            if !has_matching_directive {
                anyhow::bail!("line {line_number} does not contain a valid run directive");
            }
        }
    }

    // Run compile test if requested
    // TODO: Implement compile test in Phase 4
    if test_file.test_types.contains(&parse::TestType::Compile) {
        // test_compile::run_compile_test(...)?;
    }

    // Run transform test if requested
    // TODO: Implement transform test in Phase 4
    if test_file
        .test_types
        .contains(&parse::TestType::TransformQ32)
    {
        // test_transform::run_transform_q32_test(...)?;
    }

    // Run error test if requested
    if test_file.test_types.contains(&parse::TestType::Error) {
        let (result, stats, unexpected_pass_lines, failed_lines) =
            test_error::run_error_test(&test_file, path)?;
        return Ok((
            result,
            BTreeMap::new(),
            stats,
            unexpected_pass_lines,
            failed_lines,
            false,
        ));
    }

    // Run execution tests if requested
    if test_file
        .test_types
        .iter()
        .any(|t| matches!(t, parse::TestType::Run))
    {
        let (result, per_target, stats, unexpected_pass_lines, failed_lines, compile_failed) =
            test_run::run_test_file_with_line_filter(
                &test_file,
                path,
                line_filter,
                output_mode,
                targets,
            )?;
        Ok((
            result,
            per_target,
            stats,
            unexpected_pass_lines,
            failed_lines,
            compile_failed,
        ))
    } else {
        Ok((
            Ok(()),
            BTreeMap::new(),
            test_run::TestCaseStats::default(),
            Vec::new(),
            Vec::new(),
            false,
        ))
    }
}

/// Represents a parsed file path that may include a line number.
#[derive(Debug, Clone)]
struct FileSpec {
    path: PathBuf,
    line_number: Option<usize>,
}

/// Main entry point for `lp-glsl-filetests-app test`.
///
/// Take a list of filenames which can be either `.glsl` files or directories.
/// Files can optionally include line numbers in the format `file.glsl:42`.
/// Glob patterns are supported (e.g., `*.glsl`, `math/*`, `*add*`).
///
/// Files are interpreted as test cases and executed immediately.
///
/// Directories are scanned recursively for test cases ending in `.glsl`.
///
/// Mode is determined by test count:
/// - Single test (1 file): Full detailed output with all error information
/// - Multiple tests (>1 file): Minimal output with colored checkmarks
///
/// `fix_xfail` enables automatic removal of `[expect-fail]` markers from tests that pass.
/// Can also be enabled via `LP_FIX_XFAIL=1` environment variable.
///
/// `mark_unimplemented` adds `@unimplemented(backend=…)` to failing tests (mirrors `--fix` for the
/// opposite workflow). Use `LP_MARK_UNIMPLEMENTED=1` or `--mark-unimplemented`. Requires a single
/// `--target` (or default `jit.q32`). With `--yes`, skips the interactive confirmation.
pub fn run(
    files: &[String],
    fix_xfail: bool,
    mark_unimplemented: bool,
    mark_unimplemented_yes: bool,
    target_filter: Option<&'static Target>,
    force_summary: bool,
) -> anyhow::Result<()> {
    // Check environment variable if flag not provided
    let fix_xfail = fix_xfail
        || std::env::var("LP_FIX_XFAIL")
            .map(|v| v == "1")
            .unwrap_or(false);

    let mark_unimplemented = mark_unimplemented
        || std::env::var("LP_MARK_UNIMPLEMENTED")
            .map(|v| v == "1")
            .unwrap_or(false);

    if mark_unimplemented && !mark_unimplemented_yes {
        println!(
            "\n{}",
            colors::colorize(
                "WARNING: This will add @unimplemented(backend=…) to failing tests (file-level for whole-file compile failures in summary mode, or per // run: otherwise).",
                colors::RED
            )
        );
        println!(
            "{}",
            colors::colorize(
                "Use only when establishing a milestone baseline. Commit before running.",
                colors::YELLOW
            )
        );
        print!("\nType 'yes' to confirm: ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut confirmation = String::new();
        std::io::stdin().read_line(&mut confirmation)?;
        if confirmation.trim() != "yes" {
            anyhow::bail!(
                "Cancelled. Type 'yes' exactly to confirm, or pass --yes to skip this prompt."
            );
        }
    }
    let active_targets: Vec<&Target> = if let Some(t) = target_filter {
        vec![t]
    } else {
        DEFAULT_TARGETS.iter().collect()
    };

    if mark_unimplemented && active_targets.len() != 1 {
        anyhow::bail!(
            "--mark-unimplemented requires exactly one target; pass e.g. --target jit.q32"
        );
    }

    let filetests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let mut test_specs = Vec::new();

    // Parse all file specifications, expanding glob patterns as needed
    for file_str in files {
        let specs = parse_file_spec_with_glob(file_str, &filetests_dir)?;

        for spec in specs {
            // Handle directories by recursively finding all .glsl files
            if spec.path.is_dir() {
                for entry in WalkDir::new(&spec.path) {
                    match entry {
                        Ok(entry) => {
                            let path = entry.path();
                            if path.is_file()
                                && path.extension().and_then(|s| s.to_str()) == Some("glsl")
                            {
                                test_specs.push(FileSpec {
                                    path: path.to_path_buf(),
                                    line_number: spec.line_number,
                                });
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: error walking directory {}: {}",
                                spec.path.display(),
                                e
                            );
                        }
                    }
                }
            } else if spec.path.is_file() {
                // Validate that the path exists and is a .glsl file
                if spec.path.extension().and_then(|s| s.to_str()) == Some("glsl") {
                    test_specs.push(spec);
                } else {
                    eprintln!(
                        "Warning: {} is not a .glsl file, skipping",
                        spec.path.display()
                    );
                }
            } else {
                eprintln!("Warning: {} does not exist, skipping", spec.path.display());
            }
        }
    }

    if test_specs.is_empty() {
        anyhow::bail!("no .glsl test files found");
    }

    // Sort for deterministic output
    test_specs.sort_by(|a, b| a.path.cmp(&b.path));

    println!("Running {} test file(s)...\n", test_specs.len());

    let start_time = Instant::now();
    let output_mode = OutputMode::from_test_count(test_specs.len(), force_summary);

    // Use sequential execution for single test, concurrent for multiple tests
    if test_specs.len() == 1 {
        // Single test: run sequentially and show full details
        let spec = &test_specs[0];
        let relative_path_str = relative_path(&spec.path, &filetests_dir);
        let display_path = if let Some(line) = spec.line_number {
            format!("{relative_path_str}:{line}")
        } else {
            relative_path_str
        };

        let (_result, per_target, stats, unexpected_pass_lines, failed_lines, compile_failed) =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_filetest_with_line_filter(
                    &spec.path,
                    spec.line_number,
                    output_mode,
                    &active_targets,
                )
            })) {
                Ok(Ok((r, pt, s, up, fl, cf))) => (r, pt, s, up, fl, cf),
                Ok(Err(e)) => (
                    Err(e),
                    BTreeMap::new(),
                    test_run::TestCaseStats::default(),
                    Vec::new(),
                    Vec::new(),
                    false,
                ),
                Err(e) => {
                    let panic_msg = if let Some(msg) = e.downcast_ref::<String>() {
                        msg.clone()
                    } else if let Some(msg) = e.downcast_ref::<&'static str>() {
                        msg.to_string()
                    } else {
                        format!("{e:?}")
                    };
                    (
                        Err(anyhow::anyhow!("panicked: {panic_msg}")),
                        BTreeMap::new(),
                        test_run::TestCaseStats::default(),
                        Vec::new(),
                        Vec::new(),
                        false,
                    )
                }
            };

        // Check if file actually failed (has unexpected failures, unexpected passes, or whole-file compile failure)
        let file_actually_failed = stats.failed > 0 || stats.unexpected_pass > 0 || compile_failed;

        if !file_actually_failed {
            println!(
                "{}",
                colors::colorize(&format!("✓ {display_path}"), colors::GREEN)
            );
            let elapsed = start_time.elapsed();
            println!(
                "\n{}",
                format_results_summary(
                    stats.passed,
                    stats.failed,
                    stats.total,
                    1,
                    0,
                    elapsed,
                    stats.expected_failure(),
                    stats.unexpected_pass,
                    fix_xfail,
                    &per_target,
                )
            );

            // Remove markers if fix is enabled
            if fix_xfail && !unexpected_pass_lines.is_empty() {
                let target = active_targets.first().expect("active_targets non-empty");
                let file_update = util::file_update::FileUpdate::new(&spec.path);
                if let Err(e) = file_update.remove_file_level_annotations_matching(target) {
                    eprintln!("Warning: failed to remove file-level annotations: {e}");
                }
                let mut sorted = unexpected_pass_lines.clone();
                sorted.sort_unstable();
                for line_number in sorted {
                    if let Err(e) = file_update.remove_annotation(line_number) {
                        eprintln!(
                            "Warning: failed to remove annotation from line {line_number}: {e}"
                        );
                    }
                }
            }

            return Ok(());
        } else {
            // File failed - show error details
            println!(
                "{}",
                colors::colorize(&format!("✗ {display_path}"), colors::RED)
            );
            let elapsed = start_time.elapsed();
            println!(
                "\n{}",
                format_results_summary(
                    stats.passed,
                    stats.failed,
                    stats.total,
                    0,
                    1,
                    elapsed,
                    stats.expected_failure(),
                    stats.unexpected_pass,
                    fix_xfail,
                    &per_target,
                )
            );

            // Exit with error - show failure reason if available
            if let Err(e) = &_result {
                eprintln!("\n{e}");
            }
            if stats.unexpected_pass > 0 {
                if fix_xfail {
                    let target = active_targets.first().expect("active_targets non-empty");
                    let file_update = util::file_update::FileUpdate::new(&spec.path);
                    if let Err(e) = file_update.remove_file_level_annotations_matching(target) {
                        eprintln!("Warning: failed to remove file-level annotations: {e}");
                    }
                    let mut sorted = unexpected_pass_lines.clone();
                    sorted.sort_unstable();
                    for line_number in sorted {
                        let _ = file_update.remove_annotation(line_number);
                    }
                    anyhow::bail!(
                        "{} test case(s) marked [expect-fail] are now passing. Markers removed.",
                        stats.unexpected_pass
                    );
                } else {
                    anyhow::bail!(
                        "{} test case(s) marked [expect-fail] are now passing.\nTo fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers.",
                        stats.unexpected_pass
                    );
                }
            }
            if mark_unimplemented {
                let target = *active_targets.first().expect("active_targets non-empty");
                let use_file_level = matches!(output_mode, OutputMode::Summary);
                let needs_mark = compile_failed || !failed_lines.is_empty();
                if needs_mark {
                    let n = mark_unimplemented_expectations_for_file(
                        &spec.path,
                        &failed_lines,
                        compile_failed,
                        use_file_level,
                        target,
                    )?;
                    if n > 0 {
                        println!(
                            "\n{}",
                            colors::colorize(
                                "Marked baseline @unimplemented. Re-run filetests to verify green.",
                                colors::GREEN
                            )
                        );
                        return Ok(());
                    }
                    anyhow::bail!(
                        "--mark-unimplemented: failures remain but no new markers were added (already annotated?)"
                    );
                }
            }
            anyhow::bail!("1 test file(s) failed");
        }
    }

    // Multiple tests: use concurrent execution
    use runner::concurrent::ConcurrentRunner;

    #[derive(Debug)]
    enum TestState {
        New,
        Queued,
        Done,
    }

    struct TestEntry {
        spec: FileSpec,
        state: TestState,
        per_target: test_run::PerTargetStats,
        stats: test_run::TestCaseStats,
        unexpected_pass_lines: Vec<usize>,
        failed_lines: Vec<usize>,
        compile_failed: bool,
    }

    struct FailedTest {
        path: PathBuf,
        line_number: Option<usize>,
    }

    let mut tests: Vec<TestEntry> = test_specs
        .into_iter()
        .map(|spec| TestEntry {
            spec,
            state: TestState::New,
            per_target: BTreeMap::new(),
            stats: test_run::TestCaseStats::default(),
            unexpected_pass_lines: Vec::new(),
            failed_lines: Vec::new(),
            compile_failed: false,
        })
        .collect();

    let mut per_target_aggregate: BTreeMap<String, test_run::TestCaseStats> = BTreeMap::new();
    let mut concurrent_runner = ConcurrentRunner::new();
    let mut next_test = 0;
    let mut reported_tests = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut total_test_cases = 0;
    let mut passed_test_cases = 0;
    let mut failed_test_cases = 0;
    let mut expect_fail_test_cases = 0;
    let mut unexpected_pass_test_cases = 0;
    let mut failed_tests = Vec::new();

    // Queue all tests
    while next_test < tests.len() {
        let jobid = next_test;
        tests[jobid].state = TestState::Queued;
        concurrent_runner.put(
            jobid,
            &tests[jobid].spec.path,
            tests[jobid].spec.line_number,
            output_mode,
            &active_targets,
        );
        next_test += 1;
    }

    // Process replies and report results in order
    while reported_tests < tests.len() {
        // Check for completed jobs
        while let Some(reply) = concurrent_runner.try_get() {
            let runner::concurrent::Reply::Done {
                jobid,
                per_target,
                stats,
                unexpected_pass_lines,
                failed_lines,
                compile_failed,
                result: _,
            } = reply;
            tests[jobid].per_target = per_target.clone();
            tests[jobid].stats = stats;
            tests[jobid].unexpected_pass_lines = unexpected_pass_lines;
            tests[jobid].failed_lines = failed_lines;
            tests[jobid].compile_failed = compile_failed;
            tests[jobid].state = TestState::Done;
            for (name, s) in per_target {
                per_target_aggregate.entry(name).or_default().add(s);
            }
        }

        // Report next test in order if it's done
        if reported_tests < tests.len() {
            if matches!(tests[reported_tests].state, TestState::Done) {
                let spec = &tests[reported_tests].spec;
                let relative_path_str = relative_path(&spec.path, &filetests_dir);
                let display_path = if let Some(line) = spec.line_number {
                    format!("{relative_path_str}:{line}")
                } else {
                    relative_path_str
                };

                let stats = &tests[reported_tests].stats;
                let compile_failed = tests[reported_tests].compile_failed;
                total_test_cases += stats.total;
                passed_test_cases += stats.passed;
                failed_test_cases += stats.failed;
                expect_fail_test_cases += stats.expected_failure();
                unexpected_pass_test_cases += stats.unexpected_pass;

                // Determine color for counts based on pass/fail ratio
                let counts_color = if stats.total > 0 {
                    let denominator = stats.passed + stats.failed;
                    if denominator == 0 {
                        // All tests are expected failures - yellow
                        colors::YELLOW
                    } else if stats.passed == denominator {
                        // All passed - green
                        colors::GREEN
                    } else if stats.passed > 0 {
                        // Some passed - yellow
                        colors::YELLOW
                    } else {
                        // All failed - red
                        colors::RED
                    }
                } else {
                    colors::GREEN // Default to green if no test cases
                };

                let has_unexpected_failures = stats.failed > 0;
                let (counts_str, parentheticals) =
                    format_file_counts(stats, has_unexpected_failures);

                // Determine if this file actually failed (unexpected failures/passes or whole-file compile error)
                let file_actually_failed =
                    stats.failed > 0 || stats.unexpected_pass > 0 || compile_failed;

                // Choose status marker and color based on whether file actually failed
                let (status_marker, should_mark_failed) = if file_actually_failed {
                    (
                        if colors::should_color() {
                            format!("{}{}{} ", colors::RED, "✗", colors::RESET)
                        } else {
                            "✗ ".to_string()
                        },
                        true,
                    )
                } else {
                    (
                        if colors::should_color() {
                            format!("{}{}{} ", colors::GREEN, "✓", colors::RESET)
                        } else {
                            "✓ ".to_string()
                        },
                        false,
                    )
                };

                let counts_colored = if colors::should_color() && !counts_str.is_empty() {
                    format!("{}{}{}", counts_color, counts_str, colors::RESET)
                } else {
                    counts_str.clone()
                };
                let path_colored = if colors::should_color() {
                    format!(
                        "{status_marker}{counts_colored} {}{}{}{}",
                        colors::DIM,
                        display_path,
                        colors::RESET,
                        parentheticals
                    )
                } else {
                    format!("{status_marker}{counts_colored} {display_path}{parentheticals}")
                };
                println!("{path_colored}");
                // Flush stdout to ensure output appears immediately
                use std::io::Write;
                let _ = std::io::stdout().flush();

                if should_mark_failed {
                    failed += 1;
                    failed_tests.push(FailedTest {
                        path: spec.path.clone(),
                        line_number: spec.line_number,
                    });
                } else {
                    passed += 1;
                }
                reported_tests += 1;
                continue;
            }
        }

        // If we can't report the next test yet, wait for more replies
        // But first check if any more replies are available without blocking
        // This prevents unnecessary blocking when multiple tests complete quickly
        let mut got_reply = false;
        while let Some(reply) = concurrent_runner.try_get() {
            got_reply = true;
            let runner::concurrent::Reply::Done {
                jobid,
                per_target,
                stats,
                unexpected_pass_lines,
                failed_lines,
                compile_failed,
                result: _,
            } = reply;
            tests[jobid].per_target = per_target.clone();
            tests[jobid].stats = stats;
            tests[jobid].unexpected_pass_lines = unexpected_pass_lines;
            tests[jobid].failed_lines = failed_lines;
            tests[jobid].compile_failed = compile_failed;
            tests[jobid].state = TestState::Done;
            for (name, s) in per_target {
                per_target_aggregate.entry(name).or_default().add(s);
            }
        }

        // Only block if we didn't get any replies and the next test isn't done
        if !got_reply {
            if let Some(runner::concurrent::Reply::Done {
                jobid,
                per_target,
                stats,
                unexpected_pass_lines,
                failed_lines,
                compile_failed,
                result: _,
            }) = concurrent_runner.get()
            {
                tests[jobid].per_target = per_target.clone();
                tests[jobid].stats = stats;
                tests[jobid].unexpected_pass_lines = unexpected_pass_lines;
                tests[jobid].failed_lines = failed_lines;
                tests[jobid].compile_failed = compile_failed;
                tests[jobid].state = TestState::Done;
                for (name, s) in per_target {
                    per_target_aggregate.entry(name).or_default().add(s);
                }
            }
        }
    }

    // Shutdown threads
    concurrent_runner.shutdown();
    concurrent_runner.join();

    let elapsed = start_time.elapsed();

    // Print failed tests summary if there are failures
    if !failed_tests.is_empty() && !output_mode.show_full_output() {
        println!("\n{} Failed tests", failed_tests.len());
        println!("Run these commands to see test failure details\n");
        for failed_test in &failed_tests {
            let relative_path = relative_path(&failed_test.path, &filetests_dir);
            let test_path = if let Some(line) = failed_test.line_number {
                format!("{relative_path}:{line}")
            } else {
                relative_path
            };
            if colors::should_color() {
                println!(
                    "scripts/glsl-filetests.sh {}{}{}",
                    colors::DIM,
                    test_path,
                    colors::RESET
                );
            } else {
                println!("scripts/glsl-filetests.sh {test_path}");
            }
        }
    }

    // Collect all unexpected passes for marker removal
    let mut all_unexpected_passes: Vec<(PathBuf, usize)> = Vec::new();
    for test in &tests {
        for line_number in &test.unexpected_pass_lines {
            all_unexpected_passes.push((test.spec.path.clone(), *line_number));
        }
    }

    // Remove markers if fix is enabled
    if fix_xfail && !all_unexpected_passes.is_empty() {
        use std::collections::{HashMap, HashSet};
        let unique_paths: HashSet<&PathBuf> =
            all_unexpected_passes.iter().map(|(p, _)| p).collect();
        let target = active_targets.first().expect("active_targets non-empty");

        let mut file_updates: HashMap<PathBuf, util::file_update::FileUpdate> = HashMap::new();
        for path in unique_paths {
            let file_update = file_updates
                .entry(path.clone())
                .or_insert_with(|| util::file_update::FileUpdate::new(path));
            if let Err(e) = file_update.remove_file_level_annotations_matching(target) {
                eprintln!(
                    "Warning: failed to remove file-level annotations from {}: {}",
                    path.display(),
                    e
                );
            }
        }

        let mut by_path: HashMap<PathBuf, Vec<usize>> = HashMap::new();
        for (path, line_number) in &all_unexpected_passes {
            by_path.entry(path.clone()).or_default().push(*line_number);
        }
        for (path, mut line_numbers) in by_path {
            line_numbers.sort_unstable();
            let file_update = file_updates.get(&path).expect("FileUpdate exists");
            for line_number in line_numbers {
                if let Err(e) = file_update.remove_annotation(line_number) {
                    eprintln!(
                        "Warning: failed to remove annotation from {}:{}: {}",
                        path.display(),
                        line_number,
                        e
                    );
                }
            }
        }
    }

    println!(
        "\n{}",
        format_results_summary(
            passed_test_cases,
            failed_test_cases,
            total_test_cases,
            passed,
            failed,
            elapsed,
            expect_fail_test_cases,
            unexpected_pass_test_cases,
            fix_xfail,
            &per_target_aggregate,
        )
    );

    if mark_unimplemented && unexpected_pass_test_cases > 0 {
        anyhow::bail!(
            "cannot use --mark-unimplemented when there are unexpected passes; use --fix first"
        );
    }

    if mark_unimplemented {
        let target = *active_targets.first().expect("active_targets non-empty");
        let use_file_level = matches!(output_mode, OutputMode::Summary);
        let mut total_marks = 0usize;
        for test in &tests {
            let needs_mark = test.compile_failed || !test.failed_lines.is_empty();
            if !needs_mark {
                continue;
            }
            total_marks += mark_unimplemented_expectations_for_file(
                &test.spec.path,
                &test.failed_lines,
                test.compile_failed,
                use_file_level,
                target,
            )?;
        }
        if total_marks > 0 {
            println!(
                "\n{}",
                colors::colorize(
                    &format!(
                        "Applied {total_marks} baseline @unimplemented marker(s). Re-run filetests to verify green."
                    ),
                    colors::GREEN,
                )
            );
            return Ok(());
        }
        if failed > 0 {
            anyhow::bail!(
                "--mark-unimplemented: failures found but no new markers were added (already annotated?)"
            );
        }
        println!(
            "\n{}",
            colors::colorize("No failing tests needed marking.", colors::YELLOW)
        );
        return Ok(());
    }

    // Exit with error if there are unexpected failures or unexpected passes
    if failed > 0 {
        anyhow::bail!("{failed} test file(s) failed");
    }
    if unexpected_pass_test_cases > 0 {
        if fix_xfail {
            anyhow::bail!(
                "{unexpected_pass_test_cases} test case(s) marked [expect-fail] are now passing. Markers removed."
            );
        } else {
            anyhow::bail!(
                "{unexpected_pass_test_cases} test case(s) marked [expect-fail] are now passing.\nTo fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers."
            );
        }
    }

    Ok(())
}

/// Check if a string contains glob pattern characters
fn contains_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Expand glob patterns and return matching paths (files or directories)
fn expand_glob_patterns(pattern: &str, filetests_dir: &Path) -> Result<Vec<PathBuf>> {
    // Build the glob pattern - append pattern to filetests_dir
    // If pattern doesn't contain '/', it will match files/directories at the top level
    // If pattern contains '/', it will match at that specific path level
    let full_pattern = filetests_dir.join(pattern);

    // Convert to string for glob crate
    let pattern_str = full_pattern.to_string_lossy();

    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };

    let mut matches = Vec::new();
    for entry in glob_with(&pattern_str, options)? {
        match entry {
            Ok(path) => {
                // Include both files and directories - directories will be handled later
                // to recursively find all .glsl files
                if path.is_file() {
                    // Only include .glsl files
                    if path.extension().and_then(|s| s.to_str()) == Some("glsl") {
                        matches.push(path);
                    }
                } else if path.is_dir() {
                    // Include directories - they'll be expanded to find all .glsl files
                    matches.push(path);
                }
            }
            Err(e) => {
                // Log warning but continue
                eprintln!("Warning: glob pattern error: {e}");
            }
        }
    }

    // Sort for deterministic output
    matches.sort();
    Ok(matches)
}

/// Parse a file specification that may contain glob patterns and line numbers
fn parse_file_spec_with_glob(file_str: &str, filetests_dir: &Path) -> Result<Vec<FileSpec>> {
    // Check if it contains a line number (format: pattern:line_number)
    let (pattern, line_number) = if let Some(colon_pos) = file_str.find(':') {
        let (pattern_part, line_part) = file_str.split_at(colon_pos);
        let line_str = &line_part[1..]; // Skip the colon

        match line_str.parse::<usize>() {
            Ok(line) => (pattern_part, Some(line)),
            Err(_) => {
                // Not a valid line number, treat whole string as pattern
                (file_str, None)
            }
        }
    } else {
        (file_str, None)
    };

    // Check if pattern contains glob characters
    let paths = if contains_glob_pattern(pattern) {
        // Use glob to expand the pattern - this will match files and directories
        expand_glob_patterns(pattern, filetests_dir)?
    } else {
        // No glob characters - treat as literal path
        let full_path = filetests_dir.join(pattern);
        if full_path.exists() {
            vec![full_path]
        } else {
            vec![]
        }
    };

    // Create FileSpec for each matching path
    let mut specs = Vec::new();
    for path in paths {
        specs.push(FileSpec { path, line_number });
    }

    Ok(specs)
}

/// Compute relative path from filetests_dir to the given path.
fn relative_path(path: &Path, filetests_dir: &Path) -> String {
    path.strip_prefix(filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
/// Format per-file test counts with expected-fail information.
fn format_file_counts(
    stats: &test_run::TestCaseStats,
    has_unexpected_failures: bool,
) -> (String, String) {
    // Denominator excludes expected-fail tests (only count non-marked tests)
    let denominator = stats.passed + stats.failed;
    let numerator = if stats.unexpected_pass > 0 {
        // Show over 100% when there are unexpected passes
        stats.passed + stats.unexpected_pass
    } else {
        stats.passed
    };

    let counts_str = if denominator > 0 {
        format!("{numerator:2}/{denominator:2}")
    } else if stats.total > 0 {
        // No ordinary pass/fail mix: every case is @unsupported or an expected-failure
        // disposition. Show accounted/total so we do not print misleading `0/total`.
        let accounted = stats.passed + stats.unsupported + stats.expected_failure();
        format!("{accounted:2}/{total:2}", total = stats.total)
    } else {
        String::new()
    };

    // Build suffix with expected-fail/unexpected info, with colors
    let mut suffix_parts = Vec::new();
    if !has_unexpected_failures && stats.expected_failure() > 0 {
        suffix_parts.push(format!("({} expected-failure)", stats.expected_failure()));
    }
    if stats.unsupported > 0 {
        suffix_parts.push(format!("({} unsupported)", stats.unsupported));
    }
    if stats.failed > 0 {
        // Show unexpected failure count (red)
        let part = format!("({} unexpected)", stats.failed);
        if colors::should_color() {
            suffix_parts.push(format!("{}{}{}", colors::RED, part, colors::RESET));
        } else {
            suffix_parts.push(part);
        }
    }
    if stats.unexpected_pass > 0 {
        // Show unexpected pass count (green - good thing!)
        let part = format!("({} unexpected-pass)", stats.unexpected_pass);
        if colors::should_color() {
            suffix_parts.push(format!("{}{}{}", colors::GREEN, part, colors::RESET));
        } else {
            suffix_parts.push(part);
        }
    }

    let parentheticals = if suffix_parts.is_empty() {
        String::new()
    } else {
        format!(" {}", suffix_parts.join(" "))
    };

    (counts_str, parentheticals)
}

/// Format per-target table for summary. Returns empty string if no per-target data.
fn format_target_table(per_target: &BTreeMap<String, test_run::TestCaseStats>) -> String {
    if per_target.is_empty() {
        return String::new();
    }

    let with_color = colors::should_color();

    let w_name = per_target
        .keys()
        .map(|s| s.len())
        .max()
        .unwrap_or(12)
        .max(14);
    let col_pass = 6;
    let col_fail = 6;
    let col_unimpl = 7;
    let col_broken = 7;

    let mut out = String::new();

    // Header
    let header = format!(
        "{:>w_name$}  {:>col_pass$}  {:>col_fail$}  {:>col_unimpl$}  {:>col_broken$}",
        "", "pass", "fail", "unimpl", "broken"
    );
    if with_color {
        out.push_str(&format!("{}{}{}\n", colors::DIM, header, colors::RESET));
    } else {
        out.push_str(&format!("{header}\n"));
    }

    for (name, s) in per_target {
        let pass_pad = format!("{:>col_pass$}", s.passed);
        let fail_pad = format!("{:>col_fail$}", s.failed);
        let unimpl_pad = format!("{:>col_unimpl$}", s.unimplemented);
        let broken_pad = format!("{:>col_broken$}", s.broken);

        let pass_cell = if with_color {
            format!("{}{pass_pad}{}", colors::GREEN, colors::RESET)
        } else {
            pass_pad
        };

        let fail_cell = if s.failed > 0 && with_color {
            format!("{}{fail_pad}{}", colors::RED, colors::RESET)
        } else {
            fail_pad
        };

        let unimpl_cell = if s.unimplemented > 0 && with_color {
            format!("{}{unimpl_pad}{}", colors::YELLOW, colors::RESET)
        } else {
            unimpl_pad
        };

        let broken_cell = if s.broken > 0 && with_color {
            format!("{}{broken_pad}{}", colors::YELLOW, colors::RESET)
        } else {
            broken_pad
        };

        out.push_str(&format!(
            "{name:>w_name$}  {pass_cell}  {fail_cell}  {unimpl_cell}  {broken_cell}\n"
        ));
    }

    out
}

/// Format results summary with per-target table, file counts, and timing.
fn format_results_summary(
    passed_test_cases: usize,
    failed_test_cases: usize,
    _total_test_cases: usize,
    passed_files: usize,
    failed_files: usize,
    elapsed: std::time::Duration,
    expect_fail_count: usize,
    unexpected_pass_count: usize,
    fix_enabled: bool,
    per_target: &BTreeMap<String, test_run::TestCaseStats>,
) -> String {
    let seconds = elapsed.as_secs_f64();
    let time_str = if seconds < 1.0 {
        format!("{:.0}ms", elapsed.as_millis())
    } else if seconds < 60.0 {
        format!("{seconds:.2}s")
    } else {
        let mins = elapsed.as_secs() / 60;
        let remaining_secs = elapsed.as_secs_f64() - (mins * 60) as f64;
        format!("{mins}m {remaining_secs:.2}s")
    };

    // Denominator excludes expected-fail tests
    let denominator = passed_test_cases + failed_test_cases;
    let numerator = if unexpected_pass_count > 0 {
        // Show over 100% when there are unexpected passes
        passed_test_cases + unexpected_pass_count
    } else {
        passed_test_cases
    };

    let mut result = String::new();

    if !per_target.is_empty() {
        result.push_str(&format_target_table(per_target));
        result.push('\n');
    }

    if colors::should_color() {
        // Use red if there are failures, green if all passed
        let test_cases_color = if failed_test_cases > 0 || unexpected_pass_count > 0 {
            colors::RED
        } else {
            colors::GREEN
        };
        let files_color = if failed_files > 0 {
            colors::RED
        } else {
            colors::GREEN
        };

        let mut parts = Vec::new();
        if denominator > 0 {
            parts.push(format!(
                "{}{}/{} tests passed{}",
                test_cases_color,
                numerator,
                denominator,
                colors::RESET
            ));
        }
        if expect_fail_count > 0 {
            parts.push(format!("{expect_fail_count} expected-failure"));
        }
        parts.push(format!(
            "{}{}/{} files passed{}",
            files_color,
            passed_files,
            passed_files + failed_files,
            colors::RESET
        ));
        parts.push(time_str.to_string());

        let summary = parts.join(", ");
        result.push_str(&summary);
        if unexpected_pass_count > 0 {
            let removal_msg = if fix_enabled {
                format!("\n{unexpected_pass_count} tests newly pass. @unimplemented removed.")
            } else {
                format!("\n{unexpected_pass_count} tests newly pass. @unimplemented not removed.")
            };
            result.push_str(&removal_msg);
        }
        result
    } else {
        let mut parts = Vec::new();
        if denominator > 0 {
            parts.push(format!("{numerator}/{denominator} tests passed"));
        }
        if expect_fail_count > 0 {
            parts.push(format!("{expect_fail_count} expected-failure"));
        }
        parts.push(format!(
            "{}/{} files passed",
            passed_files,
            passed_files + failed_files
        ));
        parts.push(format!("in {time_str}"));

        let summary = parts.join(", ");
        result.push_str(&summary);
        if unexpected_pass_count > 0 {
            let removal_msg = if fix_enabled {
                format!("\n{unexpected_pass_count} tests newly pass. @unimplemented removed.")
            } else {
                format!("\n{unexpected_pass_count} tests newly pass. @unimplemented not removed.")
            };
            result.push_str(&removal_msg);
        }
        result
    }
}

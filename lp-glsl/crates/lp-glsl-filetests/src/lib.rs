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
pub mod test_compile;
pub mod test_run;
pub mod test_transform;
pub mod util;

use anyhow::Result;
use glob::{MatchOptions, glob_with};
use output_mode::OutputMode;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// Run a single filetest.
pub fn run_filetest(path: &Path) -> Result<()> {
    let (result, _stats, _, _) = run_filetest_with_line_filter(path, None, OutputMode::Detail)?;
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
/// Returns the result, test case statistics, and line numbers with unexpected passes.
pub fn run_filetest_with_line_filter(
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
) -> Result<(Result<()>, test_run::TestCaseStats, Vec<usize>, Vec<usize>)> {
    // Count test cases early, even if parsing fails later
    let early_stats = count_test_cases(path, line_filter);

    let test_file = match parse::parse_test_file(path) {
        Ok(tf) => tf,
        Err(e) => {
            // Return error but preserve the test case count we already computed
            return Ok((Err(e), early_stats, Vec::new(), Vec::new()));
        }
    };

    // Validate line number if provided
    if let Some(line_number) = line_filter {
        let has_matching_directive = test_file
            .run_directives
            .iter()
            .any(|directive| directive.line_number == line_number);
        if !has_matching_directive {
            anyhow::bail!("line {line_number} does not contain a valid run directive");
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

    // Run execution tests if requested
    if test_file
        .test_types
        .iter()
        .any(|t| matches!(t, parse::TestType::Run))
    {
        let (result, stats, unexpected_pass_lines, failed_lines) =
            test_run::run_test_file_with_line_filter(&test_file, path, line_filter, output_mode)?;
        Ok((result, stats, unexpected_pass_lines, failed_lines))
    } else {
        Ok((
            Ok(()),
            test_run::TestCaseStats::default(),
            Vec::new(),
            Vec::new(),
        ))
    }
}

/// Represents a parsed file path that may include a line number.
#[derive(Debug, Clone)]
struct FileSpec {
    path: PathBuf,
    line_number: Option<usize>,
}

/// Main entry point for `lp-test test`.
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
pub fn run(files: &[String], fix_xfail: bool) -> anyhow::Result<()> {
    // Check environment variable if flag not provided
    let fix_xfail = fix_xfail
        || std::env::var("LP_FIX_XFAIL")
            .map(|v| v == "1")
            .unwrap_or(false);

    // Check for baseline marking feature
    let mark_failing_expected = std::env::var("LP_MARK_FAILING_TESTS_EXPECTED")
        .map(|v| v == "1")
        .unwrap_or(false);

    if mark_failing_expected {
        // Show stern warning and require confirmation
        println!(
            "\n{}",
            colors::colorize(
                "WARNING: This will mark ALL currently failing tests with [expect-fail] markers.",
                colors::RED
            )
        );
        println!(
            "{}",
            colors::colorize(
                "This should only be done when establishing a baseline for expected-fail tracking.",
                colors::YELLOW
            )
        );
        println!();
        println!(
            "{}",
            colors::colorize(
                "This operation will modify test files. Make sure you have committed your changes.",
                colors::YELLOW
            )
        );
        print!("\nType 'yes' to confirm: ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut confirmation = String::new();
        std::io::stdin().read_line(&mut confirmation)?;
        let confirmation = confirmation.trim();

        if confirmation != "yes" {
            anyhow::bail!("Baseline marking cancelled. Type 'yes' exactly to confirm.");
        }
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
    let output_mode = OutputMode::from_test_count(test_specs.len());

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

        let (_result, stats, unexpected_pass_lines, _failed_lines) =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_filetest_with_line_filter(&spec.path, spec.line_number, output_mode)
            })) {
                Ok(Ok((inner_result, inner_stats, unexpected_lines, failed_lines))) => {
                    (inner_result, inner_stats, unexpected_lines, failed_lines)
                }
                Ok(Err(e)) => (
                    Err(e),
                    test_run::TestCaseStats::default(),
                    Vec::new(),
                    Vec::new(),
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
                        test_run::TestCaseStats::default(),
                        Vec::new(),
                        Vec::new(),
                    )
                }
            };

        // Check if file actually failed (has unexpected failures or unexpected passes)
        let file_actually_failed = stats.failed > 0 || stats.unexpected_pass > 0;

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
                    stats.expect_fail,
                    stats.unexpected_pass,
                    fix_xfail,
                )
            );

            // Remove markers if fix is enabled
            if fix_xfail && !unexpected_pass_lines.is_empty() {
                let file_update = util::file_update::FileUpdate::new(&spec.path);
                for line_number in unexpected_pass_lines {
                    if let Err(e) = file_update.remove_expect_fail_marker(line_number) {
                        eprintln!(
                            "Warning: failed to remove [expect-fail] marker from line {line_number}: {e}"
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
                    stats.expect_fail,
                    stats.unexpected_pass,
                    fix_xfail,
                )
            );

            // Exit with error - check for unexpected passes first
            if stats.unexpected_pass > 0 {
                if fix_xfail {
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
        stats: test_run::TestCaseStats,
        unexpected_pass_lines: Vec<usize>,
        failed_lines: Vec<usize>,
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
            stats: test_run::TestCaseStats::default(),
            unexpected_pass_lines: Vec::new(),
            failed_lines: Vec::new(),
        })
        .collect();

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
        );
        next_test += 1;
    }

    // Process replies and report results in order
    while reported_tests < tests.len() {
        // Check for completed jobs
        while let Some(reply) = concurrent_runner.try_get() {
            match reply {
                runner::concurrent::Reply::Done {
                    jobid,
                    result: _result,
                    stats,
                    unexpected_pass_lines,
                    failed_lines,
                } => {
                    tests[jobid].stats = stats;
                    tests[jobid].unexpected_pass_lines = unexpected_pass_lines;
                    tests[jobid].failed_lines = failed_lines;
                    tests[jobid].state = TestState::Done;
                }
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
                total_test_cases += stats.total;
                passed_test_cases += stats.passed;
                failed_test_cases += stats.failed;
                expect_fail_test_cases += stats.expect_fail;
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

                // Determine if this file actually failed (has unexpected failures or unexpected passes)
                let file_actually_failed = stats.failed > 0 || stats.unexpected_pass > 0;

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
            match reply {
                runner::concurrent::Reply::Done {
                    jobid,
                    result: _result,
                    stats,
                    unexpected_pass_lines,
                    failed_lines,
                } => {
                    tests[jobid].stats = stats;
                    tests[jobid].unexpected_pass_lines = unexpected_pass_lines;
                    tests[jobid].failed_lines = failed_lines;
                    tests[jobid].state = TestState::Done;
                }
            }
        }

        // Only block if we didn't get any replies and the next test isn't done
        if !got_reply {
            if let Some(reply) = concurrent_runner.get() {
                match reply {
                    runner::concurrent::Reply::Done {
                        jobid,
                        result: _result,
                        stats,
                        unexpected_pass_lines,
                        failed_lines,
                    } => {
                        tests[jobid].stats = stats;
                        tests[jobid].unexpected_pass_lines = unexpected_pass_lines;
                        tests[jobid].failed_lines = failed_lines;
                        tests[jobid].state = TestState::Done;
                    }
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
        // Group by file to create FileUpdate instances
        use std::collections::HashMap;
        let mut file_updates: HashMap<PathBuf, util::file_update::FileUpdate> = HashMap::new();
        for (path, line_number) in &all_unexpected_passes {
            let file_update = file_updates
                .entry(path.clone())
                .or_insert_with(|| util::file_update::FileUpdate::new(path));
            if let Err(e) = file_update.remove_expect_fail_marker(*line_number) {
                eprintln!(
                    "Warning: failed to remove [expect-fail] marker from {}:{}: {}",
                    path.display(),
                    line_number,
                    e
                );
            }
        }
    }

    // Baseline marking: mark all failing tests with [expect-fail]
    if mark_failing_expected {
        // Collect specific failing directives (unexpected failures, not expected ones)
        let mut failing_directives: Vec<(PathBuf, usize)> = Vec::new();
        for test in &tests {
            for line_number in &test.failed_lines {
                failing_directives.push((test.spec.path.clone(), *line_number));
            }
        }

        if !failing_directives.is_empty() {
            println!(
                "\nMarking {} failing test directive(s) with [expect-fail]...",
                failing_directives.len()
            );
            use std::collections::HashMap;
            let mut file_updates: HashMap<PathBuf, util::file_update::FileUpdate> = HashMap::new();
            let mut total_marked = 0;

            for (file_path, line_number) in &failing_directives {
                let file_update = file_updates
                    .entry(file_path.clone())
                    .or_insert_with(|| util::file_update::FileUpdate::new(file_path));

                // Mark this specific directive that failed
                if let Err(e) = file_update.add_expect_fail_marker(*line_number) {
                    eprintln!(
                        "Warning: failed to add [expect-fail] marker to {}:{}: {}",
                        file_path.display(),
                        line_number,
                        e
                    );
                } else {
                    total_marked += 1;
                }
            }

            println!("Marked {total_marked} test directive(s) with [expect-fail]");
        } else {
            println!("\nNo failing tests to mark.");
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
        )
    );

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
        // All tests are expected failures - show 0/total in yellow
        format!("{numerator:2}/{total:2}", total = stats.total)
    } else {
        String::new()
    };

    // Build suffix with expected-fail/unexpected info, with colors
    let mut suffix_parts = Vec::new();
    if !has_unexpected_failures && stats.expect_fail > 0 {
        // Show expected-fail count if no unexpected failures (grey/default)
        suffix_parts.push(format!("({} expect-fail)", stats.expect_fail));
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

/// Format results summary with colors and timing.
fn format_results_summary(
    passed_test_cases: usize,
    failed_test_cases: usize,
    _total_test_cases: usize, // Kept for API compatibility but not used (denominator excludes expect-fail)
    passed_files: usize,
    failed_files: usize,
    elapsed: std::time::Duration,
    expect_fail_count: usize,
    unexpected_pass_count: usize,
    fix_enabled: bool,
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
            parts.push(format!("{expect_fail_count} expect-fail"));
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
        if unexpected_pass_count > 0 {
            let removal_msg = if fix_enabled {
                format!("\n{unexpected_pass_count} tests newly pass. [expect-fail] removed.")
            } else {
                format!("\n{unexpected_pass_count} tests newly pass. [expect-fail] not removed.")
            };
            format!("{summary}{removal_msg}")
        } else {
            summary
        }
    } else {
        let mut parts = Vec::new();
        if denominator > 0 {
            parts.push(format!("{numerator}/{denominator} tests passed"));
        }
        if expect_fail_count > 0 {
            parts.push(format!("{expect_fail_count} expect-fail"));
        }
        parts.push(format!(
            "{}/{} files passed",
            passed_files,
            passed_files + failed_files
        ));
        parts.push(format!("in {time_str}"));

        let summary = parts.join(", ");
        if unexpected_pass_count > 0 {
            let removal_msg = if fix_enabled {
                format!("\n{unexpected_pass_count} tests newly pass. [expect-fail] removed.")
            } else {
                format!("\n{unexpected_pass_count} tests newly pass. [expect-fail] not removed.")
            };
            format!("{summary}{removal_msg}")
        } else {
            summary
        }
    }
}

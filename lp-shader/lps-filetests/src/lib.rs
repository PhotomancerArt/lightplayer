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
pub mod targets;
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

use crate::parse::RunDirective;
use crate::targets::{
    AnnotationKind, DEFAULT_TARGETS, Disposition, Target, directive_disposition,
    parse_target_filters,
};

fn directive_has_unimplemented_for(directive: &RunDirective, for_target: &Target) -> bool {
    directive
        .annotations
        .iter()
        .any(|a| a.kind == AnnotationKind::Unimplemented && a.applies_to(for_target))
}

/// Adds `// @unimplemented(target)` before each failing `// run:` (or before each expect-success
/// run when the whole file failed to compile).
///
/// When `only_if_unimplemented_for` is `Some(baseline)`, only directives that already carry
/// `@unimplemented(<baseline>)` are considered (used to duplicate baseline markers onto a new
/// backend without touching unrelated failures).
/// Returns how many marker operations were applied (0 if already annotated).
fn mark_unimplemented_expectations_for_file(
    path: &Path,
    failed_lines: &[usize],
    compile_failed: bool,
    target: &Target,
    only_if_unimplemented_for: Option<&Target>,
) -> anyhow::Result<usize> {
    let ann = format!("// @unimplemented({})", target.name());
    let mut n = 0;

    if compile_failed {
        let tf = crate::parse::parse_test_file(path)?;
        let u = util::file_update::FileUpdate::new(path);
        let mut lines_to_mark: Vec<usize> = tf
            .run_directives
            .iter()
            .filter(|d| {
                let baseline_ok = only_if_unimplemented_for
                    .map(|b| directive_has_unimplemented_for(d, b))
                    .unwrap_or(true);
                baseline_ok
                    && matches!(
                        directive_disposition(&d.annotations, target),
                        Disposition::ExpectSuccess
                    )
            })
            .map(|d| d.line_number)
            .collect();
        lines_to_mark.sort_unstable();
        lines_to_mark.dedup();
        for line in lines_to_mark {
            if u.per_directive_unimplemented_present(line, target)? {
                continue;
            }
            u.add_annotation(line, &ann)?;
            n += 1;
        }
        return Ok(n);
    }

    let tf = if only_if_unimplemented_for.is_some() {
        Some(crate::parse::parse_test_file(path)?)
    } else {
        None
    };

    let u = util::file_update::FileUpdate::new(path);
    let mut sorted: Vec<usize> = failed_lines.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for line in sorted {
        if let (Some(tf), Some(baseline)) = (&tf, only_if_unimplemented_for) {
            let Some(d) = tf.run_directives.iter().find(|d| d.line_number == line) else {
                continue;
            };
            if !directive_has_unimplemented_for(d, baseline) {
                continue;
            }
        }
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
    let (result, _, _, _, _, _, _) =
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
/// whether any target had a whole-file compile failure.
pub fn run_filetest_with_line_filter(
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    targets: &[&Target],
) -> Result<(
    Result<()>,
    test_run::PerTargetStats,
    test_run::TestCaseStats,
    std::collections::BTreeMap<String, Vec<usize>>,
    std::collections::BTreeMap<String, Vec<usize>>,
    std::collections::BTreeMap<String, bool>,
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
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
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
        let mut up_map = BTreeMap::new();
        for t in targets {
            if !unexpected_pass_lines.is_empty() {
                up_map.insert(t.name(), unexpected_pass_lines.clone());
            }
        }
        let mut fl_map = BTreeMap::new();
        for t in targets {
            if !failed_lines.is_empty() {
                fl_map.insert(t.name(), failed_lines.clone());
            }
        }
        return Ok((
            result,
            BTreeMap::new(),
            stats,
            up_map,
            fl_map,
            BTreeMap::new(),
            false,
        ));
    }

    // Run execution tests if requested
    if test_file
        .test_types
        .iter()
        .any(|t| matches!(t, parse::TestType::Run))
    {
        let (
            result,
            per_target,
            stats,
            unexpected_pass_by_target,
            failed_lines_by_target,
            compile_failed_by_target,
            compile_failed,
        ) = test_run::run_test_file_with_line_filter(
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
            unexpected_pass_by_target,
            failed_lines_by_target,
            compile_failed_by_target,
            compile_failed,
        ))
    } else {
        Ok((
            Ok(()),
            BTreeMap::new(),
            test_run::TestCaseStats::default(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
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

/// Main entry point for `lps-filetests-app test`.
///
/// Take a list of filenames which can be either `.glsl` files or directories.
/// Files can optionally include line numbers in the format `file.glsl:42`.
/// Glob patterns are supported (e.g., `*.glsl`, `math/*`, `*add*`).
///
/// Files are interpreted as test cases and executed immediately.
///
/// Directories are scanned recursively for test cases ending in `.glsl`.
///
/// Output verbosity is resolved by [`OutputMode::resolve`]: optional `--debug` / `--concise` /
/// `--detail`, then `DEBUG=1`, then single file → detail, multiple files → concise.
///
/// `fix_xfail` enables automatic removal of `[expect-fail]` markers from tests that pass.
/// Can also be enabled via `LP_FIX_XFAIL=1` environment variable.
///
/// `mark_unimplemented` adds `// @unimplemented(target)` before failing `// run:` lines (mirrors `--fix` for the
/// opposite workflow). Use `LP_MARK_UNIMPLEMENTED=1` or `--mark-unimplemented`. Applies per active
/// target when multiple are selected. With `--yes`, skips the interactive confirmation.
///
/// `mark_unimplemented_if_baseline` (or `LP_MARK_UNIMPLEMENTED_IF_BASELINE=<target>`) restricts
/// marking to directives that already have `@unimplemented(<baseline>)`, so you can copy baseline
/// markers onto another backend (e.g. `rv32.q32` → `rv32fa.q32`) without touching unrelated failures.
/// Requires exactly one `--target`.
pub fn run(
    files: &[String],
    fix_xfail: bool,
    mark_unimplemented: bool,
    mark_unimplemented_yes: bool,
    mark_unimplemented_if_baseline: Option<String>,
    target_spec: Option<&str>,
    output_override: Option<OutputMode>,
) -> anyhow::Result<()> {
    // Check environment variable if flag not provided
    let fix_xfail = fix_xfail
        || std::env::var("LP_FIX_XFAIL")
            .map(|v| v == "1")
            .unwrap_or(false);

    let mark_unimplemented_plain = mark_unimplemented
        || std::env::var("LP_MARK_UNIMPLEMENTED")
            .map(|v| v == "1")
            .unwrap_or(false);

    let mark_if_baseline_spec = mark_unimplemented_if_baseline
        .or_else(|| std::env::var("LP_MARK_UNIMPLEMENTED_IF_BASELINE").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let baseline_for_dup: Option<&'static Target> = match mark_if_baseline_spec.as_deref() {
        Some(spec) => Some(Target::from_name(spec).map_err(|e| {
            anyhow::anyhow!(
                "invalid baseline target '{spec}' for --mark-unimplemented-if-baseline / LP_MARK_UNIMPLEMENTED_IF_BASELINE: {e}"
            )
        })?),
        None => None,
    };

    let mark_mode_active = mark_unimplemented_plain || baseline_for_dup.is_some();

    if mark_mode_active && !mark_unimplemented_yes {
        let warn = if baseline_for_dup.is_some() {
            format!(
                "WARNING: This will add // @unimplemented(<active-target>) only before failing // run: lines that already carry // @unimplemented({}).",
                baseline_for_dup.expect("baseline").name()
            )
        } else {
            "WARNING: This will add // @unimplemented(<target>) before failing // run: lines."
                .to_string()
        };
        println!("\n{}", colors::colorize(&warn, colors::RED));
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
    let active_targets: Vec<&Target> = if let Some(spec) = target_spec {
        parse_target_filters(spec).map_err(anyhow::Error::msg)?
    } else {
        DEFAULT_TARGETS.iter().collect()
    };

    if baseline_for_dup.is_some() && active_targets.len() != 1 {
        anyhow::bail!(
            "--mark-unimplemented-if-baseline requires exactly one `--target` (got {})",
            active_targets
                .iter()
                .map(|t| t.name())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let target_name_width = active_targets
        .iter()
        .map(|t| t.name().len())
        .max()
        .unwrap_or(0);

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
    let output_mode = OutputMode::resolve(
        output_override,
        OutputMode::env_wants_debug(),
        test_specs.len(),
    );

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

        let (
            _result,
            per_target,
            stats,
            unexpected_pass_by_target,
            failed_lines_by_target,
            compile_failed_by_target,
            any_compile_failed,
            harness_completed,
        ) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_filetest_with_line_filter(
                &spec.path,
                spec.line_number,
                output_mode,
                &active_targets,
            )
        })) {
            Ok(Ok((r, pt, s, up, fl, cfm, cf))) => (r, pt, s, up, fl, cfm, cf, true),
            Ok(Err(e)) => (
                Err(e),
                BTreeMap::new(),
                count_test_cases(&spec.path, spec.line_number),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                false,
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
                    count_test_cases(&spec.path, spec.line_number),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    false,
                    false,
                )
            }
        };

        // Check if file actually failed. Whole-file compile failure is already
        // reflected in `stats.failed` for each `// run:` that expected success; if every run is
        // marked `@unimplemented` / expect-fail, `failed` stays 0 and the file is still OK.
        let file_actually_failed =
            !harness_completed || stats.failed > 0 || stats.unexpected_pass > 0;

        let show_target_col = active_targets.len() > 1 && target_name_width > 0;
        let target_col_w = target_name_width.max(8);

        if !file_actually_failed {
            if show_target_col {
                for t in &active_targets {
                    let tn = t.name();
                    let tstats = per_target.get(&tn).cloned().unwrap_or_default();
                    let line_failed = per_target_file_line_failed(harness_completed, &tstats);
                    let (status_marker, _) = if line_failed {
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
                    let counts_color = if tstats.total > 0 {
                        let denominator = tstats.passed + tstats.failed;
                        if !harness_completed {
                            colors::RED
                        } else if denominator == 0 {
                            colors::YELLOW
                        } else if tstats.passed == denominator {
                            colors::GREEN
                        } else if tstats.passed > 0 {
                            colors::YELLOW
                        } else {
                            colors::RED
                        }
                    } else {
                        colors::GREEN
                    };
                    let has_unexpected_failures = tstats.failed > 0;
                    let compile_failed_t = *compile_failed_by_target.get(&tn).unwrap_or(&false);
                    let (counts_str, parentheticals) = format_file_counts(
                        &tstats,
                        has_unexpected_failures,
                        harness_completed,
                        compile_failed_t,
                    );
                    let counts_colored = if colors::should_color() && !counts_str.is_empty() {
                        format!("{}{}{}", counts_color, counts_str, colors::RESET)
                    } else {
                        counts_str.clone()
                    };
                    print_filetest_status_line(
                        &status_marker,
                        &counts_colored,
                        Some((&tn, target_col_w)),
                        &display_path,
                        &parentheticals,
                    );
                }
            } else {
                let counts_color = if stats.total > 0 {
                    let denominator = stats.passed + stats.failed;
                    if !harness_completed {
                        colors::RED
                    } else if denominator == 0 {
                        colors::YELLOW
                    } else if stats.passed == denominator {
                        colors::GREEN
                    } else if stats.passed > 0 {
                        colors::YELLOW
                    } else {
                        colors::RED
                    }
                } else {
                    colors::GREEN
                };
                let has_unexpected_failures = stats.failed > 0;
                let (counts_str, parentheticals) = format_file_counts(
                    &stats,
                    has_unexpected_failures,
                    harness_completed,
                    any_compile_failed,
                );
                let status_marker = if colors::should_color() {
                    format!("{}{}{} ", colors::GREEN, "✓", colors::RESET)
                } else {
                    "✓ ".to_string()
                };
                let counts_colored = if colors::should_color() && !counts_str.is_empty() {
                    format!("{}{}{}", counts_color, counts_str, colors::RESET)
                } else {
                    counts_str.clone()
                };
                print_filetest_status_line(
                    &status_marker,
                    &counts_colored,
                    None,
                    &display_path,
                    &parentheticals,
                );
            }
            let elapsed = start_time.elapsed();
            let compile_fail_table =
                compile_fail_counts_for_table(&per_target, &compile_failed_by_target);
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
                    &compile_fail_table,
                )
            );

            if fix_xfail && !unexpected_pass_by_target.is_empty() {
                apply_unexpected_pass_fixes(&spec.path, &unexpected_pass_by_target);
            }

            return Ok(());
        } else {
            if show_target_col {
                for t in &active_targets {
                    let tn = t.name();
                    let tstats = per_target.get(&tn).cloned().unwrap_or_default();
                    let line_failed = per_target_file_line_failed(harness_completed, &tstats);
                    let (status_marker, _) = if line_failed {
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
                    let counts_color = if tstats.total > 0 {
                        let denominator = tstats.passed + tstats.failed;
                        if !harness_completed {
                            colors::RED
                        } else if denominator == 0 {
                            colors::YELLOW
                        } else if tstats.passed == denominator {
                            colors::GREEN
                        } else if tstats.passed > 0 {
                            colors::YELLOW
                        } else {
                            colors::RED
                        }
                    } else {
                        colors::GREEN
                    };
                    let has_unexpected_failures = tstats.failed > 0;
                    let compile_failed_t = *compile_failed_by_target.get(&tn).unwrap_or(&false);
                    let (counts_str, parentheticals) = format_file_counts(
                        &tstats,
                        has_unexpected_failures,
                        harness_completed,
                        compile_failed_t,
                    );
                    let counts_colored = if colors::should_color() && !counts_str.is_empty() {
                        format!("{}{}{}", counts_color, counts_str, colors::RESET)
                    } else {
                        counts_str.clone()
                    };
                    print_filetest_status_line(
                        &status_marker,
                        &counts_colored,
                        Some((&tn, target_col_w)),
                        &display_path,
                        &parentheticals,
                    );
                }
            } else {
                let counts_color = if stats.total > 0 {
                    let denominator = stats.passed + stats.failed;
                    if !harness_completed {
                        colors::RED
                    } else if denominator == 0 {
                        colors::YELLOW
                    } else if stats.passed == denominator {
                        colors::GREEN
                    } else if stats.passed > 0 {
                        colors::YELLOW
                    } else {
                        colors::RED
                    }
                } else {
                    colors::GREEN
                };
                let has_unexpected_failures = stats.failed > 0;
                let (counts_str, parentheticals) = format_file_counts(
                    &stats,
                    has_unexpected_failures,
                    harness_completed,
                    any_compile_failed,
                );
                let status_marker = if colors::should_color() {
                    format!("{}{}{} ", colors::RED, "✗", colors::RESET)
                } else {
                    "✗ ".to_string()
                };
                let counts_colored = if colors::should_color() && !counts_str.is_empty() {
                    format!("{}{}{}", counts_color, counts_str, colors::RESET)
                } else {
                    counts_str.clone()
                };
                print_filetest_status_line(
                    &status_marker,
                    &counts_colored,
                    None,
                    &display_path,
                    &parentheticals,
                );
            }
            let elapsed = start_time.elapsed();
            let compile_fail_table =
                compile_fail_counts_for_table(&per_target, &compile_failed_by_target);
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
                    &compile_fail_table,
                )
            );

            if let Err(e) = &_result {
                eprintln!("\n{e}");
            }
            if stats.unexpected_pass > 0 {
                if fix_xfail {
                    apply_unexpected_pass_fixes(&spec.path, &unexpected_pass_by_target);
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
            if mark_mode_active {
                let mut total_marks = 0usize;
                let mut any_needs_mark = false;
                for t in &active_targets {
                    let tn = t.name();
                    let failed_lines = failed_lines_by_target
                        .get(&tn)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let compile_failed_t = *compile_failed_by_target.get(&tn).unwrap_or(&false);
                    if compile_failed_t || !failed_lines.is_empty() {
                        any_needs_mark = true;
                        total_marks += mark_unimplemented_expectations_for_file(
                            &spec.path,
                            failed_lines,
                            compile_failed_t,
                            t,
                            baseline_for_dup,
                        )?;
                    }
                }
                if total_marks > 0 {
                    println!(
                        "\n{}",
                        colors::colorize(
                            "Marked baseline @unimplemented. Re-run filetests to verify green.",
                            colors::GREEN
                        )
                    );
                    return Ok(());
                }
                if any_needs_mark {
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
        unexpected_pass_by_target: BTreeMap<String, Vec<usize>>,
        failed_lines_by_target: BTreeMap<String, Vec<usize>>,
        compile_failed_by_target: BTreeMap<String, bool>,
        /// Whether any target had whole-file compile failure (for tooling; not used for pass/fail).
        #[allow(dead_code)]
        compile_failed: bool,
        /// False when the worker did not finish `run_filetest_with_line_filter` successfully.
        harness_completed: bool,
    }

    struct FailedTest {
        path: PathBuf,
        line_number: Option<usize>,
        target: String,
    }

    let mut tests: Vec<TestEntry> = test_specs
        .into_iter()
        .map(|spec| TestEntry {
            spec,
            state: TestState::New,
            per_target: BTreeMap::new(),
            stats: test_run::TestCaseStats::default(),
            unexpected_pass_by_target: BTreeMap::new(),
            failed_lines_by_target: BTreeMap::new(),
            compile_failed_by_target: BTreeMap::new(),
            compile_failed: false,
            harness_completed: false,
        })
        .collect();

    let show_target_col = active_targets.len() > 1 && target_name_width > 0;
    let target_col_w = target_name_width.max(8);

    let mut per_target_aggregate: BTreeMap<String, test_run::TestCaseStats> = BTreeMap::new();
    let mut per_target_compile_fail_files: BTreeMap<String, usize> = BTreeMap::new();
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
                unexpected_pass_by_target,
                failed_lines_by_target,
                compile_failed_by_target,
                compile_failed,
                harness_completed,
                result: _,
            } = reply;
            for (name, cf) in &compile_failed_by_target {
                if *cf {
                    *per_target_compile_fail_files
                        .entry(name.clone())
                        .or_default() += 1;
                }
            }
            tests[jobid].per_target = per_target.clone();
            tests[jobid].stats = stats;
            tests[jobid].unexpected_pass_by_target = unexpected_pass_by_target;
            tests[jobid].failed_lines_by_target = failed_lines_by_target;
            tests[jobid].compile_failed_by_target = compile_failed_by_target;
            tests[jobid].compile_failed = compile_failed;
            tests[jobid].harness_completed = harness_completed;
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
                let harness_completed = tests[reported_tests].harness_completed;
                let per_target = &tests[reported_tests].per_target;
                let compile_failed_by_target = &tests[reported_tests].compile_failed_by_target;
                let any_compile_failed = tests[reported_tests].compile_failed;
                total_test_cases += stats.total;
                if harness_completed {
                    passed_test_cases += stats.passed;
                    failed_test_cases += stats.failed;
                    expect_fail_test_cases += stats.expected_failure();
                    unexpected_pass_test_cases += stats.unexpected_pass;
                }

                // Determine if this file actually failed (unexpected failures/passes). Whole-file
                // compile failure is folded into `stats.failed` for runs that expected success.
                let file_actually_failed =
                    !harness_completed || stats.failed > 0 || stats.unexpected_pass > 0;

                let should_mark_failed = file_actually_failed;

                if show_target_col {
                    for t in &active_targets {
                        let tn = t.name();
                        let tstats = per_target.get(&tn).cloned().unwrap_or_default();
                        let line_failed = per_target_file_line_failed(harness_completed, &tstats);
                        let status_marker = if line_failed {
                            if colors::should_color() {
                                format!("{}{}{} ", colors::RED, "✗", colors::RESET)
                            } else {
                                "✗ ".to_string()
                            }
                        } else if colors::should_color() {
                            format!("{}{}{} ", colors::GREEN, "✓", colors::RESET)
                        } else {
                            "✓ ".to_string()
                        };
                        let counts_color = if tstats.total > 0 {
                            let denominator = tstats.passed + tstats.failed;
                            if !harness_completed {
                                colors::RED
                            } else if denominator == 0 {
                                colors::YELLOW
                            } else if tstats.passed == denominator {
                                colors::GREEN
                            } else if tstats.passed > 0 {
                                colors::YELLOW
                            } else {
                                colors::RED
                            }
                        } else {
                            colors::GREEN
                        };
                        let has_unexpected_failures = tstats.failed > 0;
                        let compile_failed_t = *compile_failed_by_target.get(&tn).unwrap_or(&false);
                        let (counts_str, parentheticals) = format_file_counts(
                            &tstats,
                            has_unexpected_failures,
                            harness_completed,
                            compile_failed_t,
                        );
                        let counts_colored = if colors::should_color() && !counts_str.is_empty() {
                            format!("{}{}{}", counts_color, counts_str, colors::RESET)
                        } else {
                            counts_str.clone()
                        };
                        print_filetest_status_line(
                            &status_marker,
                            &counts_colored,
                            Some((&tn, target_col_w)),
                            &display_path,
                            &parentheticals,
                        );
                        if line_failed {
                            failed_tests.push(FailedTest {
                                path: spec.path.clone(),
                                line_number: spec.line_number,
                                target: tn,
                            });
                        }
                    }
                } else {
                    let counts_color = if stats.total > 0 {
                        let denominator = stats.passed + stats.failed;
                        if !harness_completed {
                            colors::RED
                        } else if denominator == 0 {
                            colors::YELLOW
                        } else if stats.passed == denominator {
                            colors::GREEN
                        } else if stats.passed > 0 {
                            colors::YELLOW
                        } else {
                            colors::RED
                        }
                    } else {
                        colors::GREEN
                    };
                    let has_unexpected_failures = stats.failed > 0;
                    let (counts_str, parentheticals) = format_file_counts(
                        stats,
                        has_unexpected_failures,
                        harness_completed,
                        any_compile_failed,
                    );
                    let (status_marker, _) = if file_actually_failed {
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
                    print_filetest_status_line(
                        &status_marker,
                        &counts_colored,
                        None,
                        &display_path,
                        &parentheticals,
                    );
                    // When not showing target columns, still track per-target failures
                    for t in &active_targets {
                        let tn = t.name();
                        let tstats = per_target.get(&tn).cloned().unwrap_or_default();
                        let line_failed = per_target_file_line_failed(harness_completed, &tstats);
                        if line_failed {
                            failed_tests.push(FailedTest {
                                path: spec.path.clone(),
                                line_number: spec.line_number,
                                target: tn,
                            });
                        }
                    }
                }

                if should_mark_failed {
                    failed += 1;
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
                unexpected_pass_by_target,
                failed_lines_by_target,
                compile_failed_by_target,
                compile_failed,
                harness_completed,
                result: _,
            } = reply;
            for (name, cf) in &compile_failed_by_target {
                if *cf {
                    *per_target_compile_fail_files
                        .entry(name.clone())
                        .or_default() += 1;
                }
            }
            tests[jobid].per_target = per_target.clone();
            tests[jobid].stats = stats;
            tests[jobid].unexpected_pass_by_target = unexpected_pass_by_target;
            tests[jobid].failed_lines_by_target = failed_lines_by_target;
            tests[jobid].compile_failed_by_target = compile_failed_by_target;
            tests[jobid].compile_failed = compile_failed;
            tests[jobid].harness_completed = harness_completed;
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
                unexpected_pass_by_target,
                failed_lines_by_target,
                compile_failed_by_target,
                compile_failed,
                harness_completed,
                result: _,
            }) = concurrent_runner.get()
            {
                for (name, cf) in &compile_failed_by_target {
                    if *cf {
                        *per_target_compile_fail_files
                            .entry(name.clone())
                            .or_default() += 1;
                    }
                }
                tests[jobid].per_target = per_target.clone();
                tests[jobid].stats = stats;
                tests[jobid].unexpected_pass_by_target = unexpected_pass_by_target;
                tests[jobid].failed_lines_by_target = failed_lines_by_target;
                tests[jobid].compile_failed_by_target = compile_failed_by_target;
                tests[jobid].compile_failed = compile_failed;
                tests[jobid].harness_completed = harness_completed;
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
                    "scripts/glsl-filetests.sh --target {} {}{}{}",
                    failed_test.target,
                    colors::DIM,
                    test_path,
                    colors::RESET,
                );
            } else {
                println!(
                    "scripts/glsl-filetests.sh --target {} {test_path}",
                    failed_test.target
                );
            }
        }
    }

    if fix_xfail {
        for test in &tests {
            if !test.unexpected_pass_by_target.is_empty() {
                apply_unexpected_pass_fixes(&test.spec.path, &test.unexpected_pass_by_target);
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
            &per_target_compile_fail_files,
        )
    );

    if mark_mode_active && unexpected_pass_test_cases > 0 {
        anyhow::bail!(
            "cannot use --mark-unimplemented when there are unexpected passes; use --fix first"
        );
    }

    if mark_mode_active {
        let mut total_marks = 0usize;
        for test in &tests {
            for t in &active_targets {
                let tn = t.name();
                let failed_lines = test
                    .failed_lines_by_target
                    .get(&tn)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                let compile_failed_t = *test.compile_failed_by_target.get(&tn).unwrap_or(&false);
                if !compile_failed_t && failed_lines.is_empty() {
                    continue;
                }
                total_marks += mark_unimplemented_expectations_for_file(
                    &test.spec.path,
                    failed_lines,
                    compile_failed_t,
                    t,
                    baseline_for_dup,
                )?;
            }
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

fn per_target_file_line_failed(harness_completed: bool, tstats: &test_run::TestCaseStats) -> bool {
    !harness_completed || tstats.failed > 0 || tstats.unexpected_pass > 0
}

fn print_filetest_status_line(
    status_marker: &str,
    counts_colored: &str,
    target_column: Option<(&str, usize)>,
    display_path: &str,
    parentheticals: &str,
) {
    let body = if let Some((name, width)) = target_column {
        if colors::should_color() {
            format!(
                "{status_marker}{counts_colored} {name:>width$}  {}{}{}{}",
                colors::DIM,
                display_path,
                colors::RESET,
                parentheticals
            )
        } else {
            format!(
                "{status_marker}{counts_colored} {name:>width$}  {display_path}{parentheticals}"
            )
        }
    } else if colors::should_color() {
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
    println!("{body}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

fn apply_unexpected_pass_fixes(
    path: &Path,
    unexpected_pass_by_target: &BTreeMap<String, Vec<usize>>,
) {
    let file_update = util::file_update::FileUpdate::new(path);

    let mut events: Vec<(usize, &str)> = Vec::new();
    for (target_name, lines) in unexpected_pass_by_target {
        for &ln in lines {
            events.push((ln, target_name.as_str()));
        }
    }
    events.sort_by_key(|(ln, _)| *ln);

    let mut i = 0;
    while i < events.len() {
        let line = events[i].0;
        let mut group: Vec<&str> = Vec::new();
        while i < events.len() && events[i].0 == line {
            group.push(events[i].1);
            i += 1;
        }
        group.sort_unstable();
        group.dedup();
        for tn in group {
            let Ok(target) = Target::from_name(tn) else {
                continue;
            };
            if let Err(e) = file_update.remove_annotation_matching_target(line, target) {
                eprintln!("Warning: failed to remove annotation from line {line}: {e}");
            }
        }
    }
}

/// Per-target compile-fail counts for the summary table (single-file: 0 or 1 per target).
fn compile_fail_counts_for_table(
    per_target: &BTreeMap<String, test_run::TestCaseStats>,
    compile_failed_by_target: &BTreeMap<String, bool>,
) -> BTreeMap<String, usize> {
    per_target
        .keys()
        .map(|name| {
            let n = if compile_failed_by_target.get(name).copied().unwrap_or(false) {
                1
            } else {
                0
            };
            (name.clone(), n)
        })
        .collect()
}

fn format_decimal_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Smallest positive `guest_instructions_total` among targets (for relative perf column).
fn min_positive_guest_instructions(
    per_target: &BTreeMap<String, test_run::TestCaseStats>,
) -> Option<u64> {
    per_target
        .values()
        .map(|s| s.guest_instructions_total)
        .filter(|&t| t > 0)
        .min()
}

/// Multiplier vs fastest target with instruction data; `—` when this target has no counts.
fn format_inst_vs_fastest(total: u64, fastest: u64) -> String {
    if total == 0 || fastest == 0 {
        return "—".to_string();
    }
    let ratio = total as f64 / fastest as f64;
    if ratio <= 1.0005 {
        "1.00×".to_string()
    } else {
        format!("{ratio:.2}×")
    }
}

/// Color for summary perf columns: fastest = green, within 20% = yellow, else red.
fn perf_summary_color(total: u64, fastest: u64) -> Option<&'static str> {
    if total == 0 || fastest == 0 {
        return None;
    }
    let ratio = total as f64 / fastest as f64;
    if ratio <= 1.0005 {
        Some(colors::GREEN)
    } else if ratio <= 1.2 {
        Some(colors::YELLOW)
    } else {
        Some(colors::RED)
    }
}

/// Format per-file test counts with expected-fail information.
fn format_file_counts(
    stats: &test_run::TestCaseStats,
    has_unexpected_failures: bool,
    harness_completed: bool,
    whole_file_compile_failed: bool,
) -> (String, String) {
    let counts_str = if !harness_completed && stats.total > 0 {
        // Worker used `count_test_cases` only (panic or `run_filetest` error): avoid `0/total`.
        format!("--/{total:2}", total = stats.total)
    } else if stats.total > 0 {
        // Always `passed / total` over every `// run:` line. Using `passed + failed` as the
        // denominator made the same file show different totals per target in some edge cases.
        format!("{:2}/{:2}", stats.passed, stats.total)
    } else {
        String::new()
    };

    // Build suffix with expected-fail/unexpected info, with colors
    let mut suffix_parts = Vec::new();
    if whole_file_compile_failed {
        let part = "(compile-fail)";
        suffix_parts.push(if colors::should_color() {
            format!("{}{}{}", colors::YELLOW, part, colors::RESET)
        } else {
            part.to_string()
        });
    }
    if !has_unexpected_failures {
        // Show breakdown of expected failures by type
        let mut ef_parts = Vec::new();
        if stats.unimplemented > 0 {
            ef_parts.push(format!("{} unimplemented", stats.unimplemented));
        }
        if stats.unsupported > 0 {
            ef_parts.push(format!("{} unsupported", stats.unsupported));
        }
        if !ef_parts.is_empty() {
            suffix_parts.push(format!("({})", ef_parts.join(", ")));
        }
    } else if stats.unsupported > 0 {
        // Show unsupported count when there are unexpected failures too
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
    if !harness_completed && stats.total > 0 {
        suffix_parts.push("(harness error; LP_FILETESTS_HARNESS_LOG=1)".to_string());
    }
    if stats.guest_instructions_total > 0 {
        let part = format!(
            "({} inst)",
            format_decimal_with_commas(stats.guest_instructions_total)
        );
        suffix_parts.push(if colors::should_color() {
            format!("{}{}{}", colors::BLUE, part, colors::RESET)
        } else {
            part
        });
    }

    let parentheticals = if suffix_parts.is_empty() {
        String::new()
    } else {
        format!(" {}", suffix_parts.join(" "))
    };

    (counts_str, parentheticals)
}

/// Format per-target table for summary. Returns empty string if no per-target data.
fn format_target_table(
    per_target: &BTreeMap<String, test_run::TestCaseStats>,
    compile_fail_files: &BTreeMap<String, usize>,
) -> String {
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
    let col_unsupported = 11;
    let col_compile_fail = 12;

    let fastest_inst = min_positive_guest_instructions(per_target);
    let show_perf = fastest_inst.is_some();
    let fastest = fastest_inst.unwrap_or(0);

    let (col_sigma_inst, col_vs_fast) = if show_perf {
        let mut w_inst = "total inst".len();
        let mut w_rel = "vs fastest".len();
        for s in per_target.values() {
            let inst_cell = if s.guest_instructions_total > 0 {
                format!(
                    "{} inst",
                    format_decimal_with_commas(s.guest_instructions_total)
                )
            } else {
                "—".to_string()
            };
            w_inst = w_inst.max(inst_cell.len());
            let rel_cell = format_inst_vs_fastest(s.guest_instructions_total, fastest);
            w_rel = w_rel.max(rel_cell.len());
        }
        (w_inst, w_rel)
    } else {
        (0usize, 0usize)
    };

    let mut out = String::new();

    // Header
    let header = if show_perf {
        format!(
            "{:>w_name$}  {:>col_pass$}  {:>col_fail$}  {:>col_unimpl$}  {:>col_unsupported$}  {:>col_compile_fail$}  {:>col_sigma_inst$}  {:>col_vs_fast$}",
            "", "pass", "fail", "unimpl", "unsupported", "compile-fail", "total inst", "vs fastest"
        )
    } else {
        format!(
            "{:>w_name$}  {:>col_pass$}  {:>col_fail$}  {:>col_unimpl$}  {:>col_unsupported$}  {:>col_compile_fail$}",
            "", "pass", "fail", "unimpl", "unsupported", "compile-fail"
        )
    };
    if with_color {
        out.push_str(&format!("{}{}{}\n", colors::DIM, header, colors::RESET));
    } else {
        out.push_str(&format!("{header}\n"));
    }

    for (name, s) in per_target {
        let cf = compile_fail_files.get(name).copied().unwrap_or(0);
        let pass_pad = format!("{:>col_pass$}", s.passed);
        let fail_pad = format!("{:>col_fail$}", s.failed);
        let unimpl_pad = format!("{:>col_unimpl$}", s.unimplemented);
        let unsupported_pad = format!("{:>col_unsupported$}", s.unsupported);
        let compile_fail_pad = format!("{:>col_compile_fail$}", cf);

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

        let unsupported_cell = if s.unsupported > 0 && with_color {
            format!("{}{unsupported_pad}{}", colors::YELLOW, colors::RESET)
        } else {
            unsupported_pad
        };

        let compile_fail_cell = if cf > 0 && with_color {
            format!("{}{compile_fail_pad}{}", colors::YELLOW, colors::RESET)
        } else {
            compile_fail_pad
        };

        if show_perf {
            let inst_cell = if s.guest_instructions_total > 0 {
                format!(
                    "{} inst",
                    format_decimal_with_commas(s.guest_instructions_total)
                )
            } else {
                "—".to_string()
            };
            let inst_padded = format!("{:>col_sigma_inst$}", inst_cell);
            let rel_cell = format_inst_vs_fastest(s.guest_instructions_total, fastest);
            let rel_padded = format!("{:>col_vs_fast$}", rel_cell);

            let color = perf_summary_color(s.guest_instructions_total, fastest);
            let inst_cell_out = if with_color {
                if let Some(c) = color {
                    format!("{}{inst_padded}{}", c, colors::RESET)
                } else {
                    inst_padded
                }
            } else {
                inst_padded
            };
            let rel_cell_out = if with_color {
                if let Some(c) = color {
                    format!("{}{rel_padded}{}", c, colors::RESET)
                } else {
                    rel_padded
                }
            } else {
                rel_padded
            };

            out.push_str(&format!(
                "{name:>w_name$}  {pass_cell}  {fail_cell}  {unimpl_cell}  {unsupported_cell}  {compile_fail_cell}  {inst_cell_out}  {rel_cell_out}\n"
            ));
        } else {
            out.push_str(&format!(
                "{name:>w_name$}  {pass_cell}  {fail_cell}  {unimpl_cell}  {unsupported_cell}  {compile_fail_cell}\n"
            ));
        }
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
    compile_fail_files: &BTreeMap<String, usize>,
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
        result.push_str(&format_target_table(per_target, compile_fail_files));
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

#[cfg(test)]
mod format_summary_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn format_file_counts_compile_failed_before_unimplemented() {
        let mut stats = test_run::TestCaseStats::default();
        stats.total = 6;
        stats.unimplemented = 6;
        let (_, parentheticals) = format_file_counts(&stats, false, true, true);
        let cf = parentheticals.find("compile-fail").expect("compile-fail");
        let un = parentheticals
            .find("6 unimplemented")
            .expect("6 unimplemented");
        assert!(cf < un, "parentheticals={parentheticals:?}");
    }

    #[test]
    fn compile_fail_counts_for_table_maps_targets() {
        let mut per_target: BTreeMap<String, test_run::TestCaseStats> = BTreeMap::new();
        per_target.insert("wasm.q32".to_string(), test_run::TestCaseStats::default());
        let mut cf: BTreeMap<String, bool> = BTreeMap::new();
        cf.insert("wasm.q32".to_string(), true);
        let m = compile_fail_counts_for_table(&per_target, &cf);
        assert_eq!(m.get("wasm.q32").copied(), Some(1));
    }

    #[test]
    fn format_inst_vs_fastest_one_is_baseline() {
        assert_eq!(format_inst_vs_fastest(100, 100), "1.00×");
        assert_eq!(format_inst_vs_fastest(0, 100), "—");
    }

    #[test]
    fn format_inst_vs_fastest_ratio() {
        assert_eq!(format_inst_vs_fastest(138, 100), "1.38×");
    }

    #[test]
    fn perf_summary_color_tiers() {
        assert_eq!(perf_summary_color(0, 100), None);
        assert_eq!(perf_summary_color(100, 100), Some(colors::GREEN));
        assert_eq!(perf_summary_color(100, 0), None);
        assert_eq!(perf_summary_color(120, 100), Some(colors::YELLOW));
        assert_eq!(perf_summary_color(121, 100), Some(colors::RED));
    }

    #[test]
    fn format_target_table_includes_perf_when_any_guest_inst() {
        let mut a = test_run::TestCaseStats::default();
        a.passed = 3;
        a.guest_instructions_total = 178;
        let mut b = test_run::TestCaseStats::default();
        b.passed = 3;
        b.guest_instructions_total = 246;
        let mut c = test_run::TestCaseStats::default();
        c.passed = 3;
        let mut per_target: BTreeMap<String, test_run::TestCaseStats> = BTreeMap::new();
        per_target.insert("rv32.q32".to_string(), a);
        per_target.insert("rv32lp.q32".to_string(), b);
        per_target.insert("wasm.q32".to_string(), c);
        let cf: BTreeMap<String, usize> = BTreeMap::new();
        let table = format_target_table(&per_target, &cf);
        assert!(
            table.contains("total inst"),
            "expected total inst column: {table}"
        );
        assert!(
            table.contains("vs fastest"),
            "expected vs fastest column: {table}"
        );
        assert!(table.contains("178 inst"));
        assert!(table.contains("246 inst"));
        assert!(table.contains("1.00×"));
        assert!(table.contains("1.38×"));
    }
}

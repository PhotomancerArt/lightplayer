//! Summary mode: compile once, reuse emulator.

use crate::parse::TestFile;
use crate::target::{AnnotationKind, Disposition, Target, directive_disposition};
use crate::test_run::TestCaseStats;
use crate::test_run::compile;
use crate::test_run::execution;
use crate::test_run::parse_assert;
use crate::test_run::record_result;
use anyhow::Result;
use lp_riscv_emu::LogLevel;
use std::path::Path;

use crate::util::format_glsl_value;

/// Run tests in summary mode: compile all functions once and reuse the same emulator.
/// Returns result, stats, unexpected-pass lines, failed lines, and whether whole-file compilation
/// failed before any `// run:` executed.
pub fn run(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    target: &Target,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>, bool)> {
    let filetests_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let relative_path = path
        .strip_prefix(&filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let mut stats = TestCaseStats::default();

    // Check file-level annotations first - if the whole file is marked as unimplemented,
    // broken, or unsupported for this target, skip compilation entirely.
    for ann in &test_file.annotations {
        if ann.filter.matches(target) {
            match ann.kind {
                AnnotationKind::Unsupported => {
                    // Count all directives as unsupported
                    for directive in &test_file.run_directives {
                        if let Some(filter_line) = line_filter {
                            if directive.line_number != filter_line {
                                continue;
                            }
                        }
                        stats.total += 1;
                        stats.unsupported += 1;
                    }
                    return Ok((
                        Ok(()),
                        stats,
                        Vec::new(),
                        Vec::new(),
                        false,
                    ));
                }
                AnnotationKind::Unimplemented | AnnotationKind::Broken => {
                    // Count all directives as unimplemented/broken
                    for directive in &test_file.run_directives {
                        if let Some(filter_line) = line_filter {
                            if directive.line_number != filter_line {
                                continue;
                            }
                        }
                        stats.total += 1;
                        if ann.kind == AnnotationKind::Unimplemented {
                            stats.unimplemented += 1;
                        } else {
                            stats.broken += 1;
                        }
                    }
                    return Ok((
                        Ok(()),
                        stats,
                        Vec::new(),
                        Vec::new(),
                        false,
                    ));
                }
            }
        }
    }

    let log_level = LogLevel::None;
    let mut executable = match compile::compile_for_target(
        &test_file.glsl_source,
        target,
        &relative_path,
        log_level,
    ) {
        Ok(exec) => exec,
        Err(e) => {
            let mut failed_lines = Vec::new();
            let mut unimplemented_count = 0;
            let mut broken_count = 0;
            let mut unsupported_count = 0;
            for directive in &test_file.run_directives {
                if let Some(filter_line) = line_filter {
                    if directive.line_number != filter_line {
                        continue;
                    }
                }
                stats.total += 1;
                let disposition =
                    directive_disposition(&test_file.annotations, &directive.annotations, target);
                match &disposition {
                    Disposition::Skip => unsupported_count += 1,
                    Disposition::ExpectFailure(AnnotationKind::Unsupported) => {
                        unsupported_count += 1;
                    }
                    Disposition::ExpectFailure(AnnotationKind::Unimplemented) => {
                        unimplemented_count += 1;
                    }
                    Disposition::ExpectFailure(AnnotationKind::Broken) => {
                        broken_count += 1;
                    }
                    Disposition::ExpectSuccess => failed_lines.push(directive.line_number),
                }
            }
            stats.failed = failed_lines.len();
            stats.unimplemented = unimplemented_count;
            stats.broken = broken_count;
            stats.unsupported = unsupported_count;
            stats.passed = 0;
            return Ok((
                Err(anyhow::anyhow!(
                    "Compilation failed for test file {relative_path}:\n\n{e}"
                )),
                stats,
                Vec::new(),
                failed_lines,
                true,
            ));
        }
    };

    let mut first_error: Option<anyhow::Error> = None;
    let mut unexpected_pass_lines = Vec::new();
    let mut failed_lines = Vec::new();

    for directive in &test_file.run_directives {
        if let Some(filter_line) = line_filter {
            if directive.line_number != filter_line {
                continue;
            }
        }

        stats.total += 1;
        let disposition =
            directive_disposition(&test_file.annotations, &directive.annotations, target);
        if disposition == Disposition::Skip {
            record_result(
                disposition,
                false,
                &mut stats,
                &mut failed_lines,
                &mut unexpected_pass_lines,
                directive.line_number,
            );
            continue;
        }

        let trap_expectation = test_file.trap_expectations.iter().find(|exp| {
            exp.line_number == directive.line_number || exp.line_number == directive.line_number + 1
        });

        let (func_name, arg_strings) =
            match parse_assert::parse_function_call(&directive.expression_str) {
                Ok(parsed) => parsed,
                Err(e) => {
                    record_result(
                        disposition,
                        false,
                        &mut stats,
                        &mut failed_lines,
                        &mut unexpected_pass_lines,
                        directive.line_number,
                    );
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(
                            "failed to parse function call at line {}: {}",
                            directive.line_number,
                            e
                        ));
                    }
                    continue;
                }
            };

        let args = match parse_assert::parse_function_arguments(&arg_strings) {
            Ok(parsed) => parsed,
            Err(e) => {
                record_result(
                    disposition,
                    false,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                if first_error.is_none() {
                    first_error = Some(anyhow::anyhow!(
                        "failed to parse function arguments at line {}: {}",
                        directive.line_number,
                        e
                    ));
                }
                continue;
            }
        };

        let execution_result = execution::execute_function(&mut *executable, &func_name, &args);

        match (execution_result, trap_expectation) {
            (Ok(_actual_value), Some(_exp)) => {
                record_result(
                    disposition,
                    false,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                if first_error.is_none() {
                    first_error = Some(anyhow::anyhow!(
                        "run test failed at line {}: expected trap but execution succeeded",
                        directive.line_number
                    ));
                }
            }
            (Err(e), None) => {
                let error_str = format!("{e:#}");
                let is_trap = error_str.contains("Trap:")
                    || error_str.contains("trap")
                    || error_str.contains("execution trapped");

                if is_trap {
                    record_result(
                        disposition,
                        false,
                        &mut stats,
                        &mut failed_lines,
                        &mut unexpected_pass_lines,
                        directive.line_number,
                    );
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(
                            "run test failed at line {}: unexpected trap",
                            directive.line_number
                        ));
                    }
                } else {
                    record_result(
                        disposition,
                        false,
                        &mut stats,
                        &mut failed_lines,
                        &mut unexpected_pass_lines,
                        directive.line_number,
                    );
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
            (Err(_e), Some(exp)) => {
                let error_str = format!("{_e:#}");

                if let Some(expected_code) = exp.trap_code {
                    if !error_str.contains(&format!("user{expected_code}")) {
                        record_result(
                            disposition,
                            false,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                        if first_error.is_none() {
                            first_error = Some(anyhow::anyhow!(
                                "run test failed at line {}: trap code mismatch (expected {}, got {})",
                                directive.line_number,
                                expected_code,
                                error_str
                            ));
                        }
                        continue;
                    }
                }

                if let Some(ref expected_msg) = exp.trap_message {
                    if !error_str.contains(expected_msg) {
                        record_result(
                            disposition,
                            false,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                        if first_error.is_none() {
                            first_error = Some(anyhow::anyhow!(
                                "run test failed at line {}: trap message mismatch",
                                directive.line_number
                            ));
                        }
                        continue;
                    }
                }

                record_result(
                    disposition,
                    true,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
            }
            (Ok(actual_value), None) => {
                let expected_value = match parse_assert::parse_glsl_value(&directive.expected_str) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        record_result(
                            disposition,
                            false,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                        if first_error.is_none() {
                            first_error = Some(anyhow::anyhow!(
                                "failed to parse expected value at line {}: {}",
                                directive.line_number,
                                e
                            ));
                        }
                        continue;
                    }
                };

                match parse_assert::compare_results(
                    &actual_value,
                    &expected_value,
                    directive.comparison,
                    directive.tolerance,
                ) {
                    Ok(()) => {
                        record_result(
                            disposition,
                            true,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                    }
                    Err(_err_msg) => {
                        record_result(
                            disposition,
                            false,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                        if first_error.is_none() {
                            first_error = Some(anyhow::anyhow!(
                                "run test failed at line {}: expected {}, got {}",
                                directive.line_number,
                                format_glsl_value(&expected_value),
                                format_glsl_value(&actual_value)
                            ));
                        }
                    }
                }
            }
        }
    }

    let result = if stats.failed > 0 || stats.unexpected_pass > 0 {
        if stats.unexpected_pass > 0 {
            Err(anyhow::anyhow!(
                "{} test case(s) marked expected-failure are now passing",
                stats.unexpected_pass
            ))
        } else {
            Err(first_error
                .unwrap_or_else(|| anyhow::anyhow!("{} test case(s) failed", stats.failed)))
        }
    } else {
        Ok(())
    };

    Ok((result, stats, unexpected_pass_lines, failed_lines, false))
}

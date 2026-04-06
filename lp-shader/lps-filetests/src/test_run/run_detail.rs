//! Detail mode: compile the full file once, then run each `// run:` directive.

use crate::output_mode::OutputMode;
use crate::parse::{RunDirective, TestFile};
use crate::targets::{AnnotationKind, Disposition, Target, directive_disposition};
use crate::test_run::TestCaseStats;
use lps_exec::GlslExecutable;

use crate::test_run::compile;
use crate::test_run::execution;
use crate::test_run::parse_assert;
use crate::test_run::record_result;
use anyhow::Result;
use lp_riscv_emu::LogLevel;
use std::path::Path;

use crate::colors;
use crate::util::format_glsl_value;

/// Run tests in detail mode: compile the full translation unit once, then execute each directive.
/// Returns result, stats, unexpected-pass lines, failed lines, and whether whole-file compilation
/// failed before any `// run:` executed.
pub fn run(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    target: &Target,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>, bool)> {
    // Compute relative path for rerun command
    let filetests_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let relative_path = path
        .strip_prefix(&filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let test_line_filter = line_filter;
    let log_level = if output_mode.show_full_output() {
        LogLevel::Instructions
    } else {
        LogLevel::None
    };

    // TODO: Implement bless mode when needed
    // let bless_enabled = env::var("CRANELIFT_TEST_BLESS").unwrap_or_default() == "1";
    // let file_update = FileUpdate::new(path);

    let mut stats = TestCaseStats::default();

    let mut errors = Vec::new();
    let mut unexpected_pass_lines = Vec::new();
    let mut failed_lines = Vec::new();

    // If every selected `// run:` is `@unsupported` for this target, skip compilation (same idea
    // as the old file-level `@unsupported`, but without a separate attachment model).
    let mut eligible_runs = 0usize;
    let mut all_unsupported = true;
    for directive in &test_file.run_directives {
        if let Some(filter_line) = test_line_filter {
            if directive.line_number != filter_line {
                continue;
            }
        }
        eligible_runs += 1;
        if directive_disposition(&directive.annotations, target) != Disposition::Skip {
            all_unsupported = false;
        }
    }
    if eligible_runs > 0 && all_unsupported {
        for directive in &test_file.run_directives {
            if let Some(filter_line) = test_line_filter {
                if directive.line_number != filter_line {
                    continue;
                }
            }
            stats.total += 1;
            stats.unsupported += 1;
            eprintln_detail_skipped_run_directive(
                output_mode,
                &relative_path,
                directive,
                target,
                "unsupported",
            );
        }
        return Ok((Ok(()), stats, Vec::new(), Vec::new(), false));
    }

    let mut executable = match compile::compile_for_target(
        &test_file.glsl_source,
        target,
        &relative_path,
        log_level,
    ) {
        Ok(exec) => exec,
        Err(e) => {
            let mut compile_failed_lines = Vec::new();
            let mut unimplemented_count = 0;
            let mut unsupported_count = 0;
            for directive in &test_file.run_directives {
                if let Some(filter_line) = test_line_filter {
                    if directive.line_number != filter_line {
                        continue;
                    }
                }
                stats.total += 1;
                let disposition = directive_disposition(&directive.annotations, target);
                match &disposition {
                    Disposition::Skip => unsupported_count += 1,
                    Disposition::ExpectFailure(AnnotationKind::Unsupported) => {
                        unsupported_count += 1;
                    }
                    Disposition::ExpectFailure(AnnotationKind::Unimplemented) => {
                        unimplemented_count += 1;
                    }
                    Disposition::ExpectSuccess => compile_failed_lines.push(directive.line_number),
                }
            }
            stats.failed = compile_failed_lines.len();
            stats.unimplemented = unimplemented_count;
            stats.unsupported = unsupported_count;
            stats.passed = 0;
            let compile_err = format!("Compilation failed for test file {relative_path}:\n\n{e:#}");
            // In Detail/Debug, print the compiler error on stderr; concise multi-file runs rely on
            // per-file `(compile-fail)` parentheticals and the summary table instead.
            if output_mode.show_full_output()
                && compile_failed_lines.is_empty()
                && (unimplemented_count > 0 || unsupported_count > 0)
            {
                eprintln!("{compile_err}");
            }
            return Ok((
                Err(anyhow::anyhow!(compile_err)),
                stats,
                Vec::new(),
                compile_failed_lines,
                true,
            ));
        }
    };

    // Process each run directive (reuse one compiled executable)
    for directive in &test_file.run_directives {
        // Filter by line number if TEST_LINE is set
        if let Some(filter_line) = test_line_filter {
            if directive.line_number != filter_line {
                continue;
            }
        }

        stats.total += 1;
        let disposition = directive_disposition(&directive.annotations, target);
        if disposition == Disposition::Skip {
            record_result(
                disposition,
                false,
                &mut stats,
                &mut failed_lines,
                &mut unexpected_pass_lines,
                directive.line_number,
            );
            eprintln_detail_skipped_run_directive(
                output_mode,
                &relative_path,
                directive,
                target,
                "unsupported",
            );
            continue;
        }

        // Check if this test expects a trap
        // Trap expectations can be on the same line or the immediately following line
        let trap_expectation = test_file.trap_expectations.iter().find(|exp| {
            exp.line_number == directive.line_number || exp.line_number == directive.line_number + 1
        });

        // Parse function call from expression (e.g., "add_float(1.5, 2.5)")
        let (func_name, arg_strings) =
            match parse_assert::parse_function_call(&directive.expression_str) {
                Ok(result) => result,
                Err(e) => {
                    record_result(
                        disposition,
                        false,
                        &mut stats,
                        &mut failed_lines,
                        &mut unexpected_pass_lines,
                        directive.line_number,
                    );
                    let error_msg = format!(
                        "failed to parse function call: {}",
                        directive.expression_str
                    );
                    eprintln_if_detail(output_mode, &error_msg);
                    errors.push(e.context(error_msg));
                    continue;
                }
            };

        // Parse arguments to GlslValue
        let args = match parse_assert::parse_function_arguments(&arg_strings) {
            Ok(result) => result,
            Err(e) => {
                record_result(
                    disposition,
                    false,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                let error_msg = format!("failed to parse function arguments: {arg_strings:?}");
                eprintln_if_detail(output_mode, &error_msg);
                errors.push(e.context(error_msg));
                continue;
            }
        };

        // Execute function and get result
        // Note: execute_function already includes emulator state in the error, so we don't add it again
        let execution_result = execution::execute_function(&mut *executable, &func_name, &args);

        match (execution_result, trap_expectation) {
            (Ok(actual_value), Some(exp)) => {
                // Expected a trap but got a value
                record_result(
                    disposition,
                    false,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                let error_msg = format_error(
                    ErrorType::ExpectedTrapGotValue,
                    &format!(
                        "expected trap but execution succeeded\n\nExpected: trap{}\nActual: value {}",
                        if let Some(code) = exp.trap_code {
                            format!(" (code {code})")
                        } else if let Some(ref msg) = exp.trap_message {
                            format!(" (message containing '{msg}')")
                        } else {
                            String::new()
                        },
                        format_glsl_value(&actual_value)
                    ),
                    &relative_path,
                    directive.line_number,
                    Some(test_file.glsl_source.as_str()),
                    Some(&*executable),
                    output_mode,
                    Some(&directive.expression_str),
                    target,
                );
                eprintln_if_detail(output_mode, &error_msg);
                errors.push(anyhow::anyhow!("{error_msg}"));
                continue;
            }
            (Err(e), None) => {
                // Got an error but didn't expect one - check if it's a trap
                let error_str = format!("{e:#}");
                let is_trap = error_str.contains("Trap:")
                    || error_str.contains("trap")
                    || error_str.contains("execution trapped");

                if is_trap {
                    // Unexpected trap
                    record_result(
                        disposition,
                        false,
                        &mut stats,
                        &mut failed_lines,
                        &mut unexpected_pass_lines,
                        directive.line_number,
                    );
                    // Extract just the error message (before emulator state)
                    let error_msg = extract_error_message(&error_str);
                    let formatted_error = format_error(
                        ErrorType::UnexpectedTrap,
                        &format!(
                            "unexpected trap\n\nExpected: value\nActual: trap\n\nError details:\n{error_msg}"
                        ),
                        &relative_path,
                        directive.line_number,
                        Some(test_file.glsl_source.as_str()),
                        Some(&*executable),
                        output_mode,
                        Some(&directive.expression_str),
                        target,
                    );
                    eprintln_if_detail(output_mode, &formatted_error);
                    errors.push(anyhow::anyhow!("{formatted_error}"));
                    continue;
                } else {
                    // Other error - format through unified formatter
                    // Extract just the error message (before emulator state)
                    let error_msg = extract_error_message(&error_str);
                    record_result(
                        disposition,
                        false,
                        &mut stats,
                        &mut failed_lines,
                        &mut unexpected_pass_lines,
                        directive.line_number,
                    );
                    let formatted_error = format_error(
                        ErrorType::ExecutionTrap,
                        &error_msg,
                        &relative_path,
                        directive.line_number,
                        Some(test_file.glsl_source.as_str()),
                        Some(&*executable),
                        output_mode,
                        Some(&directive.expression_str),
                        target,
                    );
                    eprintln_if_detail(output_mode, &formatted_error);
                    errors.push(anyhow::anyhow!("{formatted_error}"));
                    continue;
                }
            }
            (Err(e), Some(exp)) => {
                // Expected a trap and got one - verify it matches
                let error_str = format!("{e:#}");
                let error_msg = extract_error_message(&error_str);

                // Check trap code if specified
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
                        let formatted_error = format_error(
                            ErrorType::TrapMismatch,
                            &format!(
                                "trap code mismatch\n\nExpected: trap code {expected_code}\nActual trap: {error_msg}"
                            ),
                            &relative_path,
                            directive.line_number,
                            Some(test_file.glsl_source.as_str()),
                            Some(&*executable),
                            output_mode,
                            Some(&directive.expression_str),
                            target,
                        );
                        eprintln_if_detail(output_mode, &formatted_error);
                        errors.push(anyhow::anyhow!("{formatted_error}"));
                        continue;
                    }
                }

                // Check trap message if specified
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
                        let formatted_error = format_error(
                            ErrorType::TrapMismatch,
                            &format!(
                                "trap message mismatch\n\nExpected: trap message containing '{expected_msg}'\nActual trap: {error_msg}"
                            ),
                            &relative_path,
                            directive.line_number,
                            Some(test_file.glsl_source.as_str()),
                            Some(&*executable),
                            output_mode,
                            Some(&directive.expression_str),
                            target,
                        );
                        eprintln_if_detail(output_mode, &formatted_error);
                        errors.push(anyhow::anyhow!("{formatted_error}"));
                        continue;
                    }
                }

                // Trap matches expectation - test passes
                record_result(
                    disposition,
                    true,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                continue;
            }
            (Ok(actual_value), None) => {
                // Normal case: expected value, got value - continue with comparison
                // Parse expected value
                let expected_value = match parse_assert::parse_glsl_value(&directive.expected_str) {
                    Ok(value) => value,
                    Err(e) => {
                        record_result(
                            disposition,
                            false,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                        let error_msg =
                            format!("failed to parse expected value: {}", directive.expected_str);
                        eprintln_if_detail(output_mode, &error_msg);
                        errors.push(e.context(error_msg));
                        continue;
                    }
                };

                // Compare results
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
                        // Print success message in detailed mode
                        if output_mode.show_full_output() {
                            use crate::{colors, colors::should_color};
                            use std::path::Path;
                            let filename_only = Path::new(&relative_path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(&relative_path)
                                .to_string();
                            let file_line = format!("{}:{}", filename_only, directive.line_number);
                            let test_expr = format!(
                                "{} ~= {}",
                                directive.expression_str,
                                format_glsl_value(&actual_value)
                            );

                            if should_color() {
                                eprintln!(
                                    "{}{}{}{}  {}{}{}",
                                    colors::LIGHT_GREEN,
                                    "✓ ",
                                    file_line,
                                    colors::RESET,
                                    colors::DIM,
                                    test_expr,
                                    colors::RESET
                                );
                            } else {
                                eprintln!("✓ {file_line}  {test_expr}");
                            }
                        }
                    }
                    Err(_err_msg) => {
                        // TODO: Implement bless mode when needed
                        // if bless_enabled {
                        //     file_update.update_run_expectation(...)?;
                        //     stats.passed += 1;
                        // } else {
                        record_result(
                            disposition,
                            false,
                            &mut stats,
                            &mut failed_lines,
                            &mut unexpected_pass_lines,
                            directive.line_number,
                        );
                        // Format the // run: line
                        let op_str = match directive.comparison {
                            crate::parse::test_type::ComparisonOp::Exact => "==",
                            crate::parse::test_type::ComparisonOp::Approx => "~=",
                        };
                        let tolerance_str = if let Some(tol) = directive.tolerance {
                            format!(" (tolerance: {tol})")
                        } else {
                            String::new()
                        };
                        let run_line = format!(
                            "// run: {} {} {}{}",
                            directive.expression_str, op_str, directive.expected_str, tolerance_str
                        );

                        // Format expected and actual values nicely
                        let expected_formatted = format_glsl_value(&expected_value);
                        let actual_formatted = format_glsl_value(&actual_value);

                        // Format error message with colors (removed redundant filename:line and "run test failed" lines)
                        let error_msg = if colors::should_color() {
                            format!(
                                "{}{}{}\n\n{}expected:{} {}\n  {}actual:{} {}",
                                colors::RED,
                                run_line,
                                colors::RESET,
                                colors::GREEN,
                                colors::RESET,
                                expected_formatted,
                                colors::RED,
                                colors::RESET,
                                actual_formatted
                            )
                        } else {
                            format!(
                                "{run_line}\n\nexpected: {expected_formatted}\n  actual: {actual_formatted}"
                            )
                        };

                        let formatted_error = format_error(
                            ErrorType::ComparisonFailure,
                            &error_msg,
                            &relative_path,
                            directive.line_number,
                            Some(test_file.glsl_source.as_str()),
                            Some(&*executable),
                            output_mode,
                            Some(&format!(
                                "{}() {} {}",
                                directive.expression_str, op_str, directive.expected_str
                            )),
                            target,
                        );
                        eprintln_if_detail(output_mode, &formatted_error);
                        errors.push(anyhow::anyhow!("{formatted_error}"));
                        // }
                    }
                }
            }
        }
    }

    // Exit with error if there are unexpected failures (regressions) or unexpected passes
    let result = if stats.failed > 0 || stats.unexpected_pass > 0 {
        if stats.unexpected_pass > 0 {
            Err(anyhow::anyhow!(
                "{} test case(s) marked [expect-fail] are now passing",
                stats.unexpected_pass
            ))
        } else {
            // Combine all errors into one message
            let error_summary = if errors.len() == 1 {
                format!("{}", errors[0])
            } else {
                let mut summary = format!("{} test case(s) failed:\n\n", stats.failed);
                for (i, err) in errors.iter().enumerate() {
                    summary.push_str(&format!("{}. {}\n", i + 1, err));
                }
                summary
            };
            Err(anyhow::anyhow!("{error_summary}"))
        }
    } else {
        Ok(())
    };

    Ok((result, stats, unexpected_pass_lines, failed_lines, false))
}

/// Error type for unified error formatting.
enum ErrorType {
    ExecutionTrap,
    ComparisonFailure,
    TrapMismatch,
    UnexpectedTrap,
    ExpectedTrapGotValue,
}

/// Format error with consistent section ordering.
/// Sections appear in this order:
/// 1. Emulator state (DEBUG mode only)
/// 2. V-code (DEBUG mode only)
/// 3. Transformed CLIF (DEBUG mode only)
/// 4. Raw CLIF (DEBUG mode only)
/// 5. Test GLSL
/// 6. Error details (error message)
/// 7. Rerun command(s) with DEBUG variant
fn format_error(
    _error_type: ErrorType,
    error_message: &str,
    filename: &str,
    line_number: usize,
    test_glsl: Option<&str>,
    executable: Option<&dyn GlslExecutable>,
    output_mode: OutputMode,
    _test_expression: Option<&str>,
    target: &Target,
) -> String {
    let mut parts = Vec::new();

    // Debug sections (only in Debug mode)
    if output_mode.show_debug_sections() {
        if let Some(exec) = executable {
            // Emulator state
            if let Some(ref emulator_state) = exec.format_emulator_state() {
                parts.push(emulator_state.clone());
            }

            // Disassembly (machine code for Cranelift, WAT for WASM)
            if let Some(ref disasm) = exec.format_disassembly() {
                parts.push(format!("=== Disassembly ===\n{disasm}"));
            }

            // V-code
            if let Some(ref vcode) = exec.format_vcode() {
                parts.push(format!("=== VCode ===\n{vcode}"));
            }

            // Transformed CLIF
            let (_original_clif, transformed_clif) = exec.format_clif_ir();
            if let Some(ref transformed) = transformed_clif {
                parts.push(format!(
                    "=== CLIF IR (AFTER transformation) ===\n{transformed}"
                ));
            }

            // Raw CLIF
            let (original_clif, _transformed_clif) = exec.format_clif_ir();
            if let Some(ref original) = original_clif {
                parts.push(format!(
                    "=== CLIF IR (BEFORE transformation) ===\n{original}"
                ));
            }
        }
    }

    // Test GLSL
    if output_mode.show_full_output() {
        if let Some(glsl) = test_glsl {
            parts.push(format_code_block(glsl));
        }
    }

    // Error details (just the error message, filename:line removed)
    parts.push(error_message.to_string());

    // Rerun
    let target_name = target.name();
    let rerun_section = if output_mode.show_full_output() {
        let rerun_title = if colors::should_color() {
            format!("{}{}{}", colors::BOLD, "Rerun this test:", colors::RESET)
        } else {
            "Rerun this test:".to_string()
        };
        let debug_title = if colors::should_color() {
            format!(
                "{}{}{}",
                colors::BOLD,
                "Rerun with debugging:",
                colors::RESET
            )
        } else {
            "Rerun with debugging:".to_string()
        };
        format!(
            "{rerun_title}\n  scripts/glsl-filetests.sh {filename}:{line_number} --target {target_name}\n\n{debug_title}\n  DEBUG=1 scripts/glsl-filetests.sh {filename}:{line_number} --target {target_name}"
        )
    } else {
        format!("scripts/glsl-filetests.sh {filename}:{line_number} --target {target_name}")
    };
    parts.push(rerun_section);

    parts.join("\n\n")
}

/// Extract just the error message part, removing emulator state and debug info.
/// Execution errors include emulator state in the error string, but we want to
/// format that separately through our unified formatter.
fn extract_error_message(error_str: &str) -> String {
    // Look for common debug section markers and truncate there
    if let Some(pos) = error_str.find("=== Emulator State ===") {
        error_str[..pos].trim().to_string()
    } else if let Some(pos) = error_str.find("=== Debug Info ===") {
        error_str[..pos].trim().to_string()
    } else {
        // No debug sections found, return as-is
        error_str.trim().to_string()
    }
}

/// Format source code as a code block with line numbers for better readability.
///
/// Trims leading and trailing whitespace-only lines. The text is usually the full
/// file GLSL source so failure output can show context without huge empty runs.
fn format_code_block(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(start);
    let trimmed: &[&str] = if end > start { &lines[start..end] } else { &[] };
    let max_line_num_width = trimmed.len().max(1).to_string().len();

    trimmed
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:width$} | {}", i + 1, line, width = max_line_num_width))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Per-`// run:` diagnostic on parse/execute/compare failure (Detail/Debug only; concise runs use
/// per-file status lines and the summary table).
fn eprintln_if_detail(output_mode: OutputMode, msg: impl std::fmt::Display) {
    if output_mode.show_full_output() {
        eprintln!("{msg}");
    }
}

/// Per-`// run:` line when the directive is not executed (file-level skip or unsupported), in
/// detail output — mirrors the value-pass line so single-target runs match multi-target output.
fn eprintln_detail_skipped_run_directive(
    output_mode: OutputMode,
    relative_path: &str,
    directive: &RunDirective,
    target: &Target,
    outcome_label: &str,
) {
    if !output_mode.show_full_output() {
        return;
    }
    let filename_only = Path::new(relative_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(relative_path)
        .to_string();
    let file_line = format!("{}:{}", filename_only, directive.line_number);
    let op_str = match directive.comparison {
        crate::parse::test_type::ComparisonOp::Exact => "==",
        crate::parse::test_type::ComparisonOp::Approx => "~=",
    };
    let tolerance_str = directive
        .tolerance
        .map(|t| format!(" (tolerance: {t})"))
        .unwrap_or_default();
    let body = format!(
        "{} {} {}{} ({}, {})",
        directive.expression_str,
        op_str,
        directive.expected_str,
        tolerance_str,
        outcome_label,
        target.name()
    );
    if colors::should_color() {
        eprintln!(
            "{}{}{}{}  {}{}{}",
            colors::YELLOW,
            "✓ ",
            file_line,
            colors::RESET,
            colors::DIM,
            body,
            colors::RESET
        );
    } else {
        eprintln!("✓ {file_line}  {body}");
    }
}

//! Detail mode: compile the full file once, then run each `// run:` directive.

use crate::output_mode::OutputMode;
use crate::parse::{RunDirective, TestFile};
use crate::targets::{AnnotationKind, Disposition, Target, directive_disposition};
use crate::test_run::TestCaseStats;

use crate::test_run::compile;
use crate::test_run::execution;
use crate::test_run::filetest_lpvm::FiletestInstance;
use crate::test_run::parse_assert;
use crate::test_run::record_result;
use crate::test_run::set_uniform;
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
    suppress_rerun: bool,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>, bool)> {
    // Compute relative path for rerun command
    let filetests_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let relative_path = path
        .strip_prefix(&filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let test_line_filter = line_filter;
    // Per-instruction emu logging is slow; enable only for DEBUG=1 / `--debug` so failures
    // include `Execution history:` from lp-riscv-emu.
    let log_level = if output_mode.show_debug_sections() {
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

    let compiled = match compile::compile_for_target(
        &test_file.glsl_source,
        target,
        &relative_path,
        log_level,
    ) {
        Ok(c) => {
            // Print compile-time debug (same building blocks as `shader-debug.sh`: LPIR,
            // interleaved allocator trace, VInst listing, full disasm) plus runtime sections later.
            if output_mode.show_debug_sections() {
                if let Some(ir) = c.lpir_module() {
                    let lpir_text = lpir::print_module(ir);
                    if !lpir_text.trim().is_empty() {
                        eprintln!("=== LPIR ===\n{lpir_text}");
                        eprintln!("────────────────────────────────────────");
                    }
                }
                if let Some(debug_info) = c.debug_info() {
                    let output = debug_info.render(None);
                    if !output.is_empty() {
                        eprintln!("=== Compile-time debug (allocator / disasm) ===\n{output}");
                        eprintln!("────────────────────────────────────────");
                    }
                }
            }
            c
        }
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
                    Disposition::ExpectFailure(AnnotationKind::Unimplemented)
                    | Disposition::ExpectFailure(AnnotationKind::Broken) => {
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

    // Process each run directive (reuse one compiled module; new instance per directive)
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

        let gfn = match compiled.get_function_signature(&func_name) {
            Some(g) => g,
            None => {
                record_result(
                    disposition,
                    false,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                let error_msg = format!("function '{func_name}' not found");
                eprintln_if_detail(output_mode, &error_msg);
                errors.push(anyhow::anyhow!(error_msg));
                continue;
            }
        };

        let mut inst = match compiled.instantiate() {
            Ok(i) => i,
            Err(e) => {
                record_result(
                    disposition,
                    false,
                    &mut stats,
                    &mut failed_lines,
                    &mut unexpected_pass_lines,
                    directive.line_number,
                );
                let msg = format!("instantiate failed: {e:#}");
                eprintln_if_detail(output_mode, &msg);
                errors.push(e.context(msg));
                continue;
            }
        };

        if let Err(e) = set_uniform::apply_set_uniforms(
            &mut inst,
            compiled.module_sig(),
            &directive.set_uniforms,
        ) {
            record_result(
                disposition,
                false,
                &mut stats,
                &mut failed_lines,
                &mut unexpected_pass_lines,
                directive.line_number,
            );
            let msg = format!("set_uniform failed: {e:#}");
            eprintln_if_detail(output_mode, &msg);
            errors.push(e.context(msg));
            continue;
        }

        // Emulator diagnostics are appended inside `execute_function` when available.
        let execution_result =
            execution::execute_function(&mut inst, target, gfn, &func_name, &args);

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
                let formatted = format_error(
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
                    output_mode,
                    Some(&directive.expression_str),
                    target,
                    suppress_rerun,
                );
                let full_error = append_debug_state_if_requested(output_mode, &inst, formatted);
                eprintln_if_detail(output_mode, &full_error);
                errors.push(anyhow::anyhow!("{full_error}"));
                continue;
            }
            (Err(e), None) => {
                // Got an error but didn't expect one - check if it's a trap
                let error_str = format!("{e:#}");
                let is_trap = error_str.contains("Trap:")
                    || error_str.contains("trap")
                    || error_str.contains("execution trapped");

                // Extract error message and debug section (if present)
                let error_msg = extract_error_message(&error_str);
                let debug_section = extract_debug_section(&error_str);

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
                    let formatted_error = format_error(
                        ErrorType::UnexpectedTrap,
                        &format!(
                            "unexpected trap\n\nExpected: value\nActual: trap\n\nError details:\n{error_msg}"
                        ),
                        &relative_path,
                        directive.line_number,
                        Some(test_file.glsl_source.as_str()),
                        output_mode,
                        Some(&directive.expression_str),
                        target,
                        suppress_rerun,
                    );
                    // Append debug section if available and in debug mode
                    let full_error = if let Some(debug) = debug_section {
                        if output_mode.show_debug_sections() {
                            format!("{formatted_error}\n\n{debug}")
                        } else {
                            formatted_error
                        }
                    } else {
                        formatted_error
                    };
                    eprintln_if_detail(output_mode, &full_error);
                    errors.push(anyhow::anyhow!("{full_error}"));
                    continue;
                } else {
                    // Other error - format through unified formatter
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
                        output_mode,
                        Some(&directive.expression_str),
                        target,
                        suppress_rerun,
                    );
                    // Append debug section if available and in debug mode
                    let full_error = if let Some(debug) = debug_section {
                        if output_mode.show_debug_sections() {
                            format!("{formatted_error}\n\n{debug}")
                        } else {
                            formatted_error
                        }
                    } else {
                        formatted_error
                    };
                    eprintln_if_detail(output_mode, &full_error);
                    errors.push(anyhow::anyhow!("{full_error}"));
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
                            output_mode,
                            Some(&directive.expression_str),
                            target,
                            suppress_rerun,
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
                            output_mode,
                            Some(&directive.expression_str),
                            target,
                            suppress_rerun,
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
                        stats.guest_instructions_total +=
                            inst.last_guest_instruction_count().unwrap_or(0);
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
                            output_mode,
                            Some(&format!(
                                "{}() {} {}",
                                directive.expression_str, op_str, directive.expected_str
                            )),
                            target,
                            suppress_rerun,
                        );
                        let full_error =
                            append_debug_state_if_requested(output_mode, &inst, formatted_error);
                        eprintln_if_detail(output_mode, &full_error);
                        errors.push(anyhow::anyhow!("{full_error}"));
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

/// Format error with consistent section ordering (optional GLSL context, rerun hints).
/// When `suppress_rerun` is true, the rerun commands are omitted (used in mark-unimplemented mode).
fn format_error(
    _error_type: ErrorType,
    error_message: &str,
    filename: &str,
    line_number: usize,
    test_glsl: Option<&str>,
    output_mode: OutputMode,
    _test_expression: Option<&str>,
    target: &Target,
    suppress_rerun: bool,
) -> String {
    let mut parts = Vec::new();

    // Test GLSL
    if output_mode.show_full_output() {
        if let Some(glsl) = test_glsl {
            parts.push(format_code_block(glsl));
        }
    }

    // Error details (just the error message, filename:line removed)
    parts.push(error_message.to_string());

    // Rerun (omitted in mark-unimplemented mode)
    if !suppress_rerun {
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
                "{rerun_title}\n  scripts/glsl-filetests.sh --target {target_name} {filename}:{line_number}\n\n{debug_title}\n  DEBUG=1 scripts/glsl-filetests.sh --target {target_name} {filename}:{line_number}"
            )
        } else {
            format!("scripts/glsl-filetests.sh --target {target_name} {filename}:{line_number}")
        };
        parts.push(rerun_section);
    }

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

/// Extract the debug section from an error string (if present).
/// Returns the debug info section including the marker, or None if not found.
fn extract_debug_section(error_str: &str) -> Option<String> {
    if let Some(pos) = error_str.find("=== Emulator State ===") {
        Some(error_str[pos..].trim().to_string())
    } else if let Some(pos) = error_str.find("=== Debug Info ===") {
        Some(error_str[pos..].trim().to_string())
    } else {
        None
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

/// When `DEBUG=1`, append guest emulator trace / state from the last call (same data as on
/// execution errors, but successful calls only stash it on the instance).
fn append_debug_state_if_requested(
    output_mode: OutputMode,
    inst: &FiletestInstance,
    message: String,
) -> String {
    if !output_mode.show_debug_sections() {
        return message;
    }
    match inst.debug_state() {
        Some(debug) if !debug.is_empty() => format!("{message}\n\n{debug}"),
        _ => message,
    }
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

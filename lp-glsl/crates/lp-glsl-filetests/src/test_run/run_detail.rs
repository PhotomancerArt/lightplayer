//! Detail mode: compile per test case.

use crate::output_mode::OutputMode;
use crate::parse::TestFile;
use crate::test_run::TestCaseStats;
use crate::test_run::execution;
use crate::test_run::parse_assert;
use crate::test_run::record_failure;
use crate::test_run::target;
use crate::test_run::test_glsl;
use anyhow::Result;
use lp_glsl_compiler::GlslOptions;
use lp_glsl_compiler::glsl_emu_riscv32_with_metadata;
use std::path::Path;

use crate::colors;
use crate::util::format_glsl_value;

/// Run tests in detail mode: compile per test case with function filtering.
/// Returns result, stats, and list of line numbers that had unexpected passes.
pub fn run(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)> {
    // Read the original file lines to pass to test glsl generation
    let file_contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", path.display(), e))?;
    let file_lines: Vec<String> = file_contents.lines().map(|s| s.to_string()).collect();

    // Compute relative path for rerun command
    let filetests_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let relative_path = path
        .strip_prefix(&filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    // Use provided line filter
    let test_line_filter = line_filter;

    // Determine target and options
    let target_str = test_file.target.as_deref().unwrap_or("riscv32.q32");
    let (run_mode, decimal_format) = target::parse_target(target_str)?;

    let options = GlslOptions {
        run_mode,
        decimal_format,
    };

    // TODO: Implement bless mode when needed
    // let bless_enabled = env::var("CRANELIFT_TEST_BLESS").unwrap_or_default() == "1";
    // let file_update = FileUpdate::new(path);

    let mut stats = TestCaseStats::default();
    let mut errors = Vec::new();
    let mut unexpected_pass_lines = Vec::new();
    let mut failed_lines = Vec::new();

    // Process each run directive
    for directive in &test_file.run_directives {
        // Filter by line number if TEST_LINE is set
        if let Some(filter_line) = test_line_filter {
            if directive.line_number != filter_line {
                continue;
            }
        }

        stats.total += 1;
        // Generate test GLSL code with span information
        let test_glsl_result = match test_glsl::generate_test_glsl(
            &file_lines,
            directive.line_number,
            &directive.expression_str,
        ) {
            Ok(result) => result,
            Err(e) => {
                record_failure(directive, &mut stats, &mut failed_lines);
                let error_msg = format!("failed to generate test GLSL: {e}");
                eprintln!("{error_msg}");
                errors.push(e.context(error_msg));
                continue;
            }
        };

        // Compile and execute
        // Note: test_glsl_result.source now contains ONLY the function being tested
        let mut executable = match glsl_emu_riscv32_with_metadata(
            &test_glsl_result.source,
            options.clone(),
            Some(relative_path.clone()),
        ) {
            Ok(exec) => exec,
            Err(e) => {
                // Compilation failed - check if this is an expected failure
                record_failure(directive, &mut stats, &mut failed_lines);
                let formatted_error = format_compilation_error(
                    &e,
                    &test_glsl_result,
                    directive.line_number,
                    &directive.expression_str,
                    &relative_path,
                    output_mode,
                );
                eprintln!("{formatted_error}");
                errors.push(anyhow::anyhow!("{formatted_error}"));
                continue;
            }
        };

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
                    record_failure(directive, &mut stats, &mut failed_lines);
                    let error_msg = format!(
                        "failed to parse function call: {}",
                        directive.expression_str
                    );
                    eprintln!("{error_msg}");
                    errors.push(e.context(error_msg));
                    continue;
                }
            };

        // Parse arguments to GlslValue
        let args = match parse_assert::parse_function_arguments(&arg_strings) {
            Ok(result) => result,
            Err(e) => {
                record_failure(directive, &mut stats, &mut failed_lines);
                let error_msg = format!("failed to parse function arguments: {arg_strings:?}");
                eprintln!("{error_msg}");
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
                record_failure(directive, &mut stats, &mut failed_lines);
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
                    Some(&test_glsl_result.source),
                    Some(&*executable),
                    output_mode,
                    Some(&directive.expression_str),
                );
                eprintln!("{error_msg}");
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
                    record_failure(directive, &mut stats, &mut failed_lines);
                    // Extract just the error message (before emulator state)
                    let error_msg = extract_error_message(&error_str);
                    let formatted_error = format_error(
                        ErrorType::UnexpectedTrap,
                        &format!(
                            "unexpected trap\n\nExpected: value\nActual: trap\n\nError details:\n{error_msg}"
                        ),
                        &relative_path,
                        directive.line_number,
                        Some(&test_glsl_result.source),
                        Some(&*executable),
                        output_mode,
                        Some(&directive.expression_str),
                    );
                    eprintln!("{formatted_error}");
                    errors.push(anyhow::anyhow!("{formatted_error}"));
                    continue;
                } else {
                    // Other error - format through unified formatter
                    // Extract just the error message (before emulator state)
                    let error_msg = extract_error_message(&error_str);
                    record_failure(directive, &mut stats, &mut failed_lines);
                    let formatted_error = format_error(
                        ErrorType::ExecutionTrap,
                        &error_msg,
                        &relative_path,
                        directive.line_number,
                        Some(&test_glsl_result.source),
                        Some(&*executable),
                        output_mode,
                        Some(&directive.expression_str),
                    );
                    eprintln!("{formatted_error}");
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
                        record_failure(directive, &mut stats, &mut failed_lines);
                        let formatted_error = format_error(
                            ErrorType::TrapMismatch,
                            &format!(
                                "trap code mismatch\n\nExpected: trap code {expected_code}\nActual trap: {error_msg}"
                            ),
                            &relative_path,
                            directive.line_number,
                            Some(&test_glsl_result.source),
                            Some(&*executable),
                            output_mode,
                            Some(&directive.expression_str),
                        );
                        eprintln!("{formatted_error}");
                        errors.push(anyhow::anyhow!("{formatted_error}"));
                        continue;
                    }
                }

                // Check trap message if specified
                if let Some(ref expected_msg) = exp.trap_message {
                    if !error_str.contains(expected_msg) {
                        record_failure(directive, &mut stats, &mut failed_lines);
                        let formatted_error = format_error(
                            ErrorType::TrapMismatch,
                            &format!(
                                "trap message mismatch\n\nExpected: trap message containing '{expected_msg}'\nActual trap: {error_msg}"
                            ),
                            &relative_path,
                            directive.line_number,
                            Some(&test_glsl_result.source),
                            Some(&*executable),
                            output_mode,
                            Some(&directive.expression_str),
                        );
                        eprintln!("{formatted_error}");
                        errors.push(anyhow::anyhow!("{formatted_error}"));
                        continue;
                    }
                }

                // Trap matches expectation - test passes
                if directive.expect_fail {
                    stats.unexpected_pass += 1;
                    unexpected_pass_lines.push(directive.line_number);
                } else {
                    stats.passed += 1;
                }
                continue;
            }
            (Ok(actual_value), None) => {
                // Normal case: expected value, got value - continue with comparison
                // Parse expected value
                let expected_value = match parse_assert::parse_glsl_value(&directive.expected_str) {
                    Ok(value) => value,
                    Err(e) => {
                        record_failure(directive, &mut stats, &mut failed_lines);
                        let error_msg =
                            format!("failed to parse expected value: {}", directive.expected_str);
                        eprintln!("{error_msg}");
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
                        // Test passed
                        if directive.expect_fail {
                            stats.unexpected_pass += 1;
                            unexpected_pass_lines.push(directive.line_number);
                        } else {
                            stats.passed += 1;
                        }
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
                        record_failure(directive, &mut stats, &mut failed_lines);
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
                            Some(&test_glsl_result.source),
                            Some(&*executable),
                            output_mode,
                            Some(&format!(
                                "{}() {} {}",
                                directive.expression_str, op_str, directive.expected_str
                            )),
                        );
                        eprintln!("{formatted_error}");
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

    Ok((result, stats, unexpected_pass_lines, failed_lines))
}

/// Error type for unified error formatting.
enum ErrorType {
    Compilation,
    ExecutionTrap,
    ComparisonFailure,
    TrapMismatch,
    UnexpectedTrap,
    ExpectedTrapGotValue,
}

/// Format a compilation error with test GLSL code context.
fn format_compilation_error(
    error: &lp_glsl_compiler::error::GlslError,
    test_glsl: &test_glsl::TestGlslResult,
    directive_line: usize,
    expression: &str,
    relative_path: &str,
    output_mode: OutputMode,
) -> anyhow::Error {
    anyhow::anyhow!(
        "{}",
        format_error(
            ErrorType::Compilation,
            &format!(
                "Compilation failed for test case at line {directive_line}:\n\nTest case: {expression}\n\n{error}"
            ),
            relative_path,
            directive_line,
            Some(&test_glsl.source),
            None, // No executable for compilation errors
            output_mode,
            Some(expression),
        )
    )
}

/// Format error with consistent section ordering.
/// Sections appear in this order:
/// 1. Emulator state (DEBUG mode only)
/// 2. V-code (DEBUG mode only)
/// 3. Transformed CLIF (DEBUG mode only)
/// 4. Raw CLIF (DEBUG mode only)
/// 5. Test GLSL (always shown)
/// 6. Error details (filename:<line>, error message)
/// 7. Rerun commands (with and without DEBUG)
fn format_error(
    _error_type: ErrorType,
    error_message: &str,
    filename: &str,
    line_number: usize,
    test_glsl: Option<&str>,
    executable: Option<&dyn lp_glsl_compiler::GlslExecutable>,
    output_mode: OutputMode,
    _test_expression: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    // Debug sections (only in Debug mode)
    if output_mode.show_debug_sections() {
        if let Some(exec) = executable {
            // Emulator state
            if let Some(ref emulator_state) = exec.format_emulator_state() {
                parts.push(emulator_state.clone());
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

    // Test GLSL (always shown if available)
    if let Some(glsl) = test_glsl {
        parts.push(format_code_block(glsl));
    }

    // Error details (just the error message, filename:line removed)
    parts.push(error_message.to_string());

    // Rerun commands
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
    let rerun_section = format!(
        "{rerun_title}\n  scripts/glsl-filetests.sh {filename}:{line_number}\n\n{debug_title}\n  DEBUG=1 scripts/glsl-filetests.sh {filename}:{line_number}"
    );
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

/// Format source code as a code block with line numbers for better readability
fn format_code_block(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let max_line_num_width = (lines.len() + 1).to_string().len();

    lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:width$} | {}", i + 1, line, width = max_line_num_width))
        .collect::<Vec<_>>()
        .join("\n")
}

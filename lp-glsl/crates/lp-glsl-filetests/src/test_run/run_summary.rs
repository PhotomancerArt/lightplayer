//! Summary mode: compile once, reuse emulator.

use crate::parse::TestFile;
use crate::test_run::TestCaseStats;
use crate::test_run::execution;
use crate::test_run::parse_assert;
use crate::test_run::record_failure;
use crate::test_run::target;
use anyhow::Result;
use lp_glsl_compiler::GlslOptions;
use lp_glsl_compiler::glsl_emu_riscv32_with_metadata;
use std::path::Path;

use crate::util::format_glsl_value;

/// Run tests in summary mode: compile all functions once and reuse the same emulator.
/// Returns result, stats, list of line numbers that had unexpected passes, and list of line numbers that failed.
pub fn run(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)> {
    // Compute relative path for rerun command
    let filetests_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let relative_path = path
        .strip_prefix(&filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    // Determine target and options
    let target_str = test_file.target.as_deref().unwrap_or("riscv32.q32");
    let (run_mode, decimal_format) = target::parse_target(target_str)?;

    let options = GlslOptions {
        run_mode,
        decimal_format,
    };

    // Count total test cases before compilation (so we can show counts even if compilation fails)
    let mut stats = TestCaseStats::default();
    for directive in &test_file.run_directives {
        if let Some(filter_line) = line_filter {
            if directive.line_number != filter_line {
                continue;
            }
        }
        stats.total += 1;
    }

    // Compile all functions from the test file once (no test glsl filtering)
    let mut executable = match glsl_emu_riscv32_with_metadata(
        &test_file.glsl_source,
        options.clone(),
        Some(relative_path.clone()),
    ) {
        Ok(exec) => exec,
        Err(e) => {
            // Compilation failed - categorize directives based on whether they're marked [expect-fail]
            // Only unmarked directives should go into failed_lines (for bless)
            // Marked directives should be counted as expect_fail
            let mut failed_lines = Vec::new();
            let mut expect_fail_count = 0;

            for directive in &test_file.run_directives {
                // Respect line filter if present
                if let Some(filter_line) = line_filter {
                    if directive.line_number != filter_line {
                        continue;
                    }
                }

                if directive.expect_fail {
                    expect_fail_count += 1;
                } else {
                    failed_lines.push(directive.line_number);
                }
            }

            stats.failed = failed_lines.len();
            stats.expect_fail = expect_fail_count;
            stats.passed = 0;
            return Ok((
                Err(anyhow::anyhow!(
                    "Compilation failed for test file {relative_path}:\n\n{e}"
                )),
                stats,
                Vec::new(),
                failed_lines,
            ));
        }
    };

    // TODO: Implement bless mode when needed
    // let bless_enabled = env::var("CRANELIFT_TEST_BLESS").unwrap_or_default() == "1";
    // let file_update = FileUpdate::new(path);

    let mut first_error: Option<anyhow::Error> = None;
    let mut unexpected_pass_lines = Vec::new();
    let mut failed_lines = Vec::new();

    // Process each run directive using the same emulator
    for directive in &test_file.run_directives {
        // Filter by line number if provided
        if let Some(filter_line) = line_filter {
            if directive.line_number != filter_line {
                continue;
            }
        }

        // Check if this test expects a trap
        let trap_expectation = test_file.trap_expectations.iter().find(|exp| {
            exp.line_number == directive.line_number || exp.line_number == directive.line_number + 1
        });

        // Parse function call from expression
        let (func_name, arg_strings) =
            match parse_assert::parse_function_call(&directive.expression_str) {
                Ok(parsed) => parsed,
                Err(e) => {
                    record_failure(directive, &mut stats, &mut failed_lines);
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

        // Parse arguments to GlslValue
        let args = match parse_assert::parse_function_arguments(&arg_strings) {
            Ok(parsed) => parsed,
            Err(e) => {
                record_failure(directive, &mut stats, &mut failed_lines);
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

        // Execute function and get result
        // Note: The emulator's call_function automatically resets the stack pointer for each call
        let execution_result = execution::execute_function(&mut *executable, &func_name, &args);

        match (execution_result, trap_expectation) {
            (Ok(_actual_value), Some(_exp)) => {
                // Expected a trap but got a value
                record_failure(directive, &mut stats, &mut failed_lines);
                if first_error.is_none() {
                    first_error = Some(anyhow::anyhow!(
                        "run test failed at line {}: expected trap but execution succeeded",
                        directive.line_number
                    ));
                }
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
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(
                            "run test failed at line {}: unexpected trap",
                            directive.line_number
                        ));
                    }
                } else {
                    // Other error - pass through
                    record_failure(directive, &mut stats, &mut failed_lines);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
            (Err(_e), Some(exp)) => {
                // Expected a trap and got one - verify it matches
                let error_str = format!("{_e:#}");

                // Check trap code if specified
                if let Some(expected_code) = exp.trap_code {
                    if !error_str.contains(&format!("user{expected_code}")) {
                        record_failure(directive, &mut stats, &mut failed_lines);
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

                // Check trap message if specified
                if let Some(ref expected_msg) = exp.trap_message {
                    if !error_str.contains(expected_msg) {
                        record_failure(directive, &mut stats, &mut failed_lines);
                        if first_error.is_none() {
                            first_error = Some(anyhow::anyhow!(
                                "run test failed at line {}: trap message mismatch",
                                directive.line_number
                            ));
                        }
                        continue;
                    }
                }

                // Trap matches expectation - test passes
                if directive.expect_fail {
                    // Unexpected pass: test marked [expect-fail] but trap matched
                    stats.unexpected_pass += 1;
                    unexpected_pass_lines.push(directive.line_number);
                } else {
                    // Normal pass
                    stats.passed += 1;
                }
            }
            (Ok(actual_value), None) => {
                // Normal case: expected value, got value - continue with comparison
                // Parse expected value
                let expected_value = match parse_assert::parse_glsl_value(&directive.expected_str) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        record_failure(directive, &mut stats, &mut failed_lines);
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
                            // Unexpected pass: test marked [expect-fail] but passed
                            stats.unexpected_pass += 1;
                            unexpected_pass_lines.push(directive.line_number);
                        } else {
                            // Normal pass
                            stats.passed += 1;
                        }
                    }
                    Err(_err_msg) => {
                        // Test failed
                        record_failure(directive, &mut stats, &mut failed_lines);
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

    // Exit with error if there are unexpected failures (regressions) or unexpected passes
    let result = if stats.failed > 0 || stats.unexpected_pass > 0 {
        if stats.unexpected_pass > 0 {
            Err(anyhow::anyhow!(
                "{} test case(s) marked [expect-fail] are now passing",
                stats.unexpected_pass
            ))
        } else {
            Err(first_error
                .unwrap_or_else(|| anyhow::anyhow!("{} test case(s) failed", stats.failed)))
        }
    } else {
        Ok(())
    };

    Ok((result, stats, unexpected_pass_lines, failed_lines))
}

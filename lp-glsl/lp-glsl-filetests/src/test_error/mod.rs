//! Error test runner: compile and match diagnostics against expected-error expectations.

use crate::parse::{ErrorExpectation, TestFile};
use crate::test_run::TestCaseStats;
use anyhow::{Result, anyhow};
use lp_glsl_compiler::GlslOptions;
use lp_glsl_compiler::glsl_emu_riscv32_with_metadata;
use lp_riscv_emu::LogLevel;
use std::path::Path;

use crate::test_run::target;

/// Run an error test: compile, expect failure, match diagnostics to expectations.
pub fn run_error_test(
    test_file: &TestFile,
    path: &Path,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)> {
    if test_file.error_expectations.is_empty() {
        return Ok((
            Err(anyhow!(
                "error test must specify at least one expected-error or expected-error-code"
            )),
            TestCaseStats::default(),
            Vec::new(),
            Vec::new(),
        ));
    }

    for exp in &test_file.error_expectations {
        if exp.message.is_none() && exp.code.is_none() {
            return Ok((
                Err(anyhow!(
                    "each expected-error must specify message and/or code (line {})",
                    exp.line
                )),
                TestCaseStats::default(),
                Vec::new(),
                Vec::new(),
            ));
        }
    }

    let target_str = test_file.target.as_deref().unwrap_or("riscv32.q32");
    let (mut run_mode, decimal_format) = target::parse_target(target_str)?;

    if let lp_glsl_compiler::RunMode::Emulator {
        ref mut log_level, ..
    } = run_mode
    {
        *log_level = Some(LogLevel::None);
    }

    let options = GlslOptions {
        run_mode,
        decimal_format,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
        memory_optimized: false,
        target_override: None,
        max_errors: lp_glsl_compiler::DEFAULT_MAX_ERRORS,
    };

    let filetests_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let relative_path = path
        .strip_prefix(&filetests_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let result = glsl_emu_riscv32_with_metadata(
        &test_file.glsl_source,
        options.clone(),
        Some(relative_path),
    );

    let mut stats = TestCaseStats::default();
    stats.total = 1;

    match result {
        Ok(_) => {
            stats.failed = 1;
            Ok((
                Err(anyhow!("expected compile error, but compilation succeeded")),
                stats,
                Vec::new(),
                vec![1],
            ))
        }
        Err(diag) => {
            let match_result =
                match_expectations_to_errors(&test_file.error_expectations, &diag.errors);

            match match_result {
                Ok(()) => {
                    stats.passed = 1;
                    Ok((Ok(()), stats, Vec::new(), Vec::new()))
                }
                Err(e) => {
                    stats.failed = 1;
                    Ok((Err(e), stats, Vec::new(), vec![1]))
                }
            }
        }
    }
}

/// Match expectations to actual errors. Returns Ok(()) if all match; Err with message otherwise.
fn match_expectations_to_errors(
    expectations: &[ErrorExpectation],
    actual_errors: &[lp_glsl_compiler::GlslError],
) -> Result<()> {
    let mut used = vec![false; actual_errors.len()];

    for exp in expectations {
        let idx = actual_errors
            .iter()
            .enumerate()
            .find(|(i, err)| {
                if used[*i] {
                    return false;
                }
                let err_line = err.location.as_ref().map(|loc| loc.line).unwrap_or(0);
                let line_match = (err_line == 0 && expectations.len() == 1) || err_line == exp.line;

                if !line_match {
                    return false;
                }

                let msg_match = exp
                    .message
                    .as_ref()
                    .map(|m| err.message.contains(m))
                    .unwrap_or(true);
                let code_match = exp
                    .code
                    .as_ref()
                    .map(|c| err.code.as_str() == c.as_str())
                    .unwrap_or(true);

                msg_match && code_match
            })
            .map(|(i, _)| i);

        let idx = match idx {
            Some(i) => i,
            None => {
                let actual_summary: String = actual_errors
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let line = e.location.as_ref().map(|l| l.line).unwrap_or(0);
                        format!(
                            "  [{}] line={} code={} msg={}",
                            i,
                            line,
                            e.code.as_str(),
                            e.message
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(anyhow!(
                    "expected error at line {} (message: {:?}, code: {:?}) but not seen.\nActual errors:\n{}",
                    exp.line,
                    exp.message,
                    exp.code,
                    actual_summary
                ));
            }
        };
        used[idx] = true;
    }

    if let Some((idx, _)) = used.iter().enumerate().find(|(_, u)| !*u) {
        let err = &actual_errors[idx];
        let line = err.location.as_ref().map(|l| l.line).unwrap_or(0);
        return Err(anyhow!(
            "unexpected error at line {}: {}",
            line,
            err.message
        ));
    }

    Ok(())
}

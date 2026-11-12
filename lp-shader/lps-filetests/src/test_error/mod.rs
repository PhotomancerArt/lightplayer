//! Error test runner: compile and match diagnostics against expected-error expectations.

use crate::parse::{ErrorExpectation, TestFile};
use crate::test_run::TestCaseStats;
use anyhow::{Result, anyhow};
use lps_diagnostics::{ErrorCode, GlFileId, GlSourceLoc, GlslError};
use lpvm_cranelift::{CompileOptions, CompilerError, FloatMode as LpirFloatMode};
use std::path::Path;

/// Run an error test: compile through `lps-glsl`, expect failure, match
/// diagnostics to expectations. Error tests run once regardless of target
/// (one front end, shared LPIR pipeline).
pub fn run_error_test(
    test_file: &TestFile,
    _path: &Path,
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

    let options = CompileOptions {
        float_mode: LpirFloatMode::Q32,
        ..Default::default()
    };

    let result = collect_glsl_error_test_diagnostics(
        &test_file.glsl_source,
        &test_file.texture_specs,
        &options,
    );

    let mut stats = TestCaseStats::default();
    stats.total = 1;

    match result {
        Ok(()) => {
            stats.failed = 1;
            Ok((
                Err(anyhow!("expected compile error, but compilation succeeded")),
                stats,
                Vec::new(),
                vec![1],
            ))
        }
        Err(errors) => {
            let match_result = match_expectations_to_errors(&test_file.error_expectations, &errors);

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

/// Compile the way the product does — `lps-glsl`, the texture-binding check,
/// then codegen — and collect what refuses the file.
///
/// `lps-glsl` diagnostics carry a byte span; the expectation matcher wants the
/// user line, so the span start is mapped to a 1-indexed line and column. They
/// all take [`ErrorCode::E0001`]: the front end does not number its errors.
/// [`ErrorCode::E0400`] stays LightPlayer's own validation — the texture
/// binding specs and codegen.
fn collect_glsl_error_test_diagnostics(
    user_source: &str,
    texture_specs: &crate::parse::test_type::TextureSpecs,
    options: &CompileOptions,
) -> Result<(), Vec<GlslError>> {
    let lp_options = lps_glsl::CompileOptions {
        texture_specs: texture_specs.clone(),
        ..Default::default()
    };
    let output = lps_glsl::compile(user_source, &lp_options)
        .map_err(|d| vec![lps_glsl_diagnostic_to_glsl(&d, user_source)])?;

    if let Err(msg) =
        lps_shared::validate_texture_binding_specs_against_module(&output.meta, texture_specs)
    {
        return Err(vec![GlslError::new(ErrorCode::E0400, msg)]);
    }

    match lpvm_cranelift::object_bytes_from_ir(&output.ir, options) {
        Ok(_) => Ok(()),
        Err(e) => Err(vec![codegen_compiler_error_to_glsl(e)]),
    }
}

fn lps_glsl_diagnostic_to_glsl(d: &lps_glsl::Diagnostic, source: &str) -> GlslError {
    let start = d.span.start.min(source.len());
    let before = &source[..start];
    let line = before.matches('\n').count() + 1;
    let column = start - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    GlslError::new(ErrorCode::E0001, d.message.clone())
        .with_location(GlSourceLoc::new(GlFileId(0), line, column))
}

fn codegen_compiler_error_to_glsl(e: CompilerError) -> GlslError {
    match e {
        CompilerError::Codegen(ce) => GlslError::new(ErrorCode::E0400, ce.to_string()),
        _ => GlslError::new(ErrorCode::E0400, e.to_string()),
    }
}

/// Match expectations to actual errors. Returns Ok(()) if all match; Err with message otherwise.
fn match_expectations_to_errors(
    expectations: &[ErrorExpectation],
    actual_errors: &[GlslError],
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
                let line_match = err_line == exp.line || (err_line == 0 && expectations.len() == 1);

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

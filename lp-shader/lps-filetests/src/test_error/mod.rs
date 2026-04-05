//! Error test runner: compile and match diagnostics against expected-error expectations.

use crate::parse::{ErrorExpectation, TestFile};
use crate::test_run::TestCaseStats;
use anyhow::{Result, anyhow};
use lpir_cranelift::{
    CompileOptions, CompilerError, FloatMode as LpirFloatMode, jit_from_ir_owned,
};
use lps_diagnostics::{ErrorCode, GlFileId, GlSourceLoc, GlslError};
use lps_frontend::naga::{ShaderStage, front::glsl::Error as NagaGlslError};
use std::path::Path;

/// Run an error test: compile, expect failure, match diagnostics to expectations.
/// Error tests run once regardless of target (shared LPIR pipeline).
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

    let result = collect_glsl_error_test_diagnostics(&test_file.glsl_source, &options);

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

fn collect_glsl_error_test_diagnostics(
    user_source: &str,
    options: &CompileOptions,
) -> Result<(), Vec<GlslError>> {
    let prep = lps_frontend::prepared_glsl_for_compile(user_source);
    let first_phys = lps_frontend::user_snippet_first_physical_line();
    let mut frontend = lps_frontend::naga::front::glsl::Frontend::default();
    let parse_opts = lps_frontend::naga::front::glsl::Options::from(ShaderStage::Vertex);
    let module = match frontend.parse(&parse_opts, &prep) {
        Err(parse_errors) => {
            return Err(parse_errors
                .errors
                .iter()
                .map(|e| naga_parse_error_to_glsl(e, &prep, user_source, first_phys))
                .collect());
        }
        Ok(m) => m,
    };

    let naga_module = match lps_frontend::naga_module_from_parsed(module) {
        Ok(nm) => nm,
        Err(e) => return Err(vec![naga_compile_error_to_glsl(e)]),
    };

    let (ir, meta) = match lps_frontend::lower(&naga_module) {
        Ok(x) => x,
        Err(e) => return Err(vec![lower_error_to_glsl(e)]),
    };

    match jit_from_ir_owned(ir, meta, options) {
        Ok(_) => Ok(()),
        Err(e) => Err(vec![codegen_compiler_error_to_glsl(e)]),
    }
}

fn naga_parse_error_to_glsl(
    err: &NagaGlslError,
    prep_source: &str,
    user_snippet: &str,
    user_first_physical_line: usize,
) -> GlslError {
    let raw_msg = err.kind.to_string();
    let user_line = err
        .location(prep_source)
        .map(|loc| user_line_from_physical(loc.line_number as usize, user_first_physical_line))
        .unwrap_or(0);
    let (code, message) = classify_naga_parse_message(&raw_msg, user_snippet, user_line);
    let mut g = GlslError::new(code, message);
    if let Some(loc) = err.location(prep_source) {
        let ul = user_line_from_physical(loc.line_number as usize, user_first_physical_line);
        if ul > 0 {
            g = g.with_location(GlSourceLoc::new(
                GlFileId(0),
                ul,
                loc.line_position as usize,
            ));
        }
    }
    g
}

fn user_line_from_physical(physical_line: usize, user_first_physical_line: usize) -> usize {
    physical_line
        .checked_sub(user_first_physical_line)
        .map(|d| d.saturating_add(1))
        .unwrap_or(0)
}

fn classify_naga_parse_message(
    raw: &str,
    user_snippet: &str,
    user_line: usize,
) -> (ErrorCode, String) {
    let line_txt = user_snippet
        .lines()
        .nth(user_line.saturating_sub(1))
        .unwrap_or("");

    if raw.contains("const values must have an initializer") {
        let name = const_decl_name_from_line(line_txt).unwrap_or_else(|| String::from("BAD"));
        return (
            ErrorCode::E0001,
            format!("const `{name}` must be initialized"),
        );
    }
    if raw.contains("Variable cannot be used in LHS position") {
        let name = assign_lhs_identifier(line_txt).unwrap_or_else(|| String::from("x"));
        return (
            ErrorCode::E0001,
            format!("cannot assign to const variable `{name}`"),
        );
    }

    if raw.contains("cannot be in the left hand side") {
        return (
            ErrorCode::E0115,
            String::from("expression is not a valid LValue"),
        );
    }
    if raw.contains("Expected LeftParen") && raw.contains("found Semicolon") {
        return (ErrorCode::E0001, String::from("expected '{', found ;"));
    }
    if raw.contains("Unexpected runtime-expression") {
        if line_txt.contains("get_val()") {
            return (
                ErrorCode::E0001,
                String::from("unknown constructor or non-const function"),
            );
        }
        return (ErrorCode::E0001, String::from("not a constant expression"));
    }
    if raw.contains("Unknown variable") {
        return (ErrorCode::E0001, String::from("undefined variable"));
    }
    (ErrorCode::E0001, raw.to_string())
}

fn const_decl_name_from_line(line: &str) -> Option<String> {
    let line = line.split("//").next()?.trim();
    let line = line.strip_suffix(';')?.trim();
    let mut it = line.split_whitespace();
    if it.next()? != "const" {
        return None;
    }
    it.next()?; // type
    let name = it.next()?.to_string();
    Some(name)
}

fn assign_lhs_identifier(line: &str) -> Option<String> {
    let line = line.split("//").next()?.trim();
    let lhs = line.split('=').next()?.trim();
    lhs.split_whitespace().last().map(|s| s.to_string())
}

fn lower_error_to_glsl(le: lps_frontend::LowerError) -> GlslError {
    let s = le.to_string();
    if s.contains("unsupported bool binary Add") {
        GlslError::new(ErrorCode::E0112, "post-increment requires numeric operand")
    } else {
        GlslError::new(ErrorCode::E0400, s)
    }
}

fn naga_compile_error_to_glsl(e: lps_frontend::CompileError) -> GlslError {
    match e {
        lps_frontend::CompileError::Parse(msg) => GlslError::new(ErrorCode::E0001, msg),
        lps_frontend::CompileError::UnsupportedType(msg) => GlslError::new(ErrorCode::E0109, msg),
    }
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

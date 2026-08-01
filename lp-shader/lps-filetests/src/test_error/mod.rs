//! Error test runner: compile and match diagnostics against expected-error expectations.

use crate::parse::{ErrorExpectation, TestFile};
use crate::targets::{Disposition, Target, directive_disposition};
use crate::test_run::TestCaseStats;
use anyhow::{Result, anyhow};
use lps_diagnostics::{ErrorCode, GlFileId, GlSourceLoc, GlslError};
use lps_frontend::naga::{ShaderStage, front::glsl::Error as NagaGlslError};
use lpvm_cranelift::{CompileOptions, CompilerError, FloatMode as LpirFloatMode};
use std::path::Path;

/// The target the `lps-glsl` arm of an error test is dispositioned against.
/// Error tests are otherwise target-agnostic; this is the canonical
/// `frontend=lp` member, so `@unimplemented(frontend=lp)` (or an exact
/// `@unimplemented(rv32lpn.q32)`) resolves the way it reads.
const LP_ERROR_TEST_TARGET: &Target = &crate::targets::DEFAULT_TARGETS[1];

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
            let match_result = match_expectations_to_errors(&test_file.error_expectations, &errors)
                .and_then(|()| lp_frontend_also_rejects(test_file));

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

/// The naga pipeline above is no longer the only frontend: `rv32lpn.q32` and
/// `xtlpn.q32` compile through `lps-glsl`, the on-device frontend. A shader
/// that only naga rejects is a frontend divergence — invalid GLSL reaching the
/// device — so an error test asserts *both* frontends refuse the file.
///
/// Only the rejection is asserted, not the message: the two frontends word
/// their diagnostics differently, and the `expected-error` expectations above
/// are written against the naga wording. `lps-glsl`'s own phrasing is pinned by
/// unit tests in that crate.
///
/// The arm is dispositioned like any other case, against the `rv32lpn.q32`
/// target — `@unimplemented(frontend=lp)` records a check `lps-glsl` does not
/// have yet, and goes stale loudly once it grows one.
fn lp_frontend_also_rejects(test_file: &TestFile) -> Result<()> {
    let disposition = directive_disposition(&test_file.file_annotations, LP_ERROR_TEST_TARGET);
    if disposition == Disposition::Skip {
        return Ok(());
    }
    let options = lps_glsl::CompileOptions {
        texture_specs: test_file.texture_specs.clone(),
        ..Default::default()
    };
    // Mirror the real `Frontend::Lp` pipeline: compile, then run the same
    // texture-binding validation `filetest_lpvm` applies to its output.
    let rejected = match lps_glsl::compile(&test_file.glsl_source, &options) {
        Err(_) => true,
        Ok(output) => lps_shared::validate_texture_binding_specs_against_module(
            &output.meta,
            &test_file.texture_specs,
        )
        .is_err(),
    };
    match (disposition, rejected) {
        (Disposition::ExpectSuccess, true) => Ok(()),
        (Disposition::ExpectSuccess, false) => Err(anyhow!(
            "the naga frontend rejects this file but the lps-glsl frontend accepts it \
             ({} would compile it) — teach lps-glsl to reject it too, or record the gap \
             with `// @unimplemented(frontend=lp)`",
            LP_ERROR_TEST_TARGET.name()
        )),
        (Disposition::ExpectFailure(_), false) => Ok(()),
        (Disposition::ExpectFailure(kind), true) => Err(anyhow!(
            "the lps-glsl frontend now rejects this file but it is annotated @{} — \
             remove the annotation",
            kind.keyword()
        )),
        (Disposition::Skip, _) => unreachable!("skip returns above"),
    }
}

fn collect_glsl_error_test_diagnostics(
    user_source: &str,
    texture_specs: &crate::parse::test_type::TextureSpecs,
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

    let lower_options = lps_frontend::LowerOptions {
        texture_specs: texture_specs.clone(),
        ..Default::default()
    };
    let (ir, meta) = match lps_frontend::lower_with_options(&naga_module, &lower_options) {
        Ok(x) => x,
        Err(e) => return Err(vec![lower_error_to_glsl(e, user_source)]),
    };

    if let Err(msg) =
        lps_shared::validate_texture_binding_specs_against_module(&meta, texture_specs)
    {
        return Err(vec![GlslError::new(ErrorCode::E0400, msg)]);
    }

    match lpvm_cranelift::object_bytes_from_ir(&ir, options) {
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

fn lower_error_to_glsl(le: lps_frontend::LowerError, user_source: &str) -> GlslError {
    let s = le.to_string();
    // naga desugars `b++` on a bool into `b = b + true`, so the only signal
    // that reaches lowering is "unsupported bool binary Add". Restore the
    // user-facing wording — but only when the source actually contains an
    // increment/decrement: written-out bool arithmetic (`bvec2 c = a + b;`)
    // produces the same lowering error and must keep its own message.
    if s.contains("unsupported bool binary Add")
        && (user_source.contains("++") || user_source.contains("--"))
    {
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

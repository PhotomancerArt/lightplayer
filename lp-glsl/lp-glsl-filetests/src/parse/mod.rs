//! Test file parsing.

pub mod parse_annotation;
pub mod parse_expected_error;
pub mod parse_run;
pub mod parse_source;
pub mod parse_target;
pub mod parse_test_type;
pub mod parse_trap;
pub mod test_type;

// Re-exports
pub use test_type::{
    ClifExpectations, ComparisonOp, ErrorExpectation, RunDirective, TestFile, TestType,
    TrapExpectation,
};

use anyhow::{Context, Result};
use std::path::Path;

/// Parse a test file and extract all directives and source code.
pub fn parse_test_file(path: &Path) -> Result<TestFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let mut test_types = Vec::new();
    let mut file_annotations = Vec::new();
    let mut run_directives = Vec::new();
    let mut trap_expectations = Vec::new();
    let mut pending_annotations: Vec<crate::target::Annotation> = Vec::new();
    let mut seen_glsl_code = false;

    for (line_num, line) in lines.iter().enumerate() {
        let line_number = line_num + 1;

        if let Some(test_type) = parse_test_type::parse_test_type(line) {
            test_types.push(test_type);
            continue;
        }

        if parse_target::parse_target_directive(line).is_some() {
            continue;
        }

        if let Ok(Some(annotation)) = parse_annotation::parse_annotation_line(line, line_number) {
            if !seen_glsl_code && run_directives.is_empty() {
                file_annotations.push(annotation);
            } else {
                pending_annotations.push(annotation);
            }
            continue;
        }

        if let Some(run_line) = parse_run::parse_run_directive_line(line) {
            let legacy_expect_fail = run_line.trim_end().ends_with("[expect-fail]");
            let mut directive =
                parse_run::parse_run_directive(run_line, line_number, legacy_expect_fail)?;
            directive.annotations = std::mem::take(&mut pending_annotations);
            run_directives.push(directive);
            continue;
        }

        if let Some(trap_exp) = parse_trap::parse_trap_expectation(line, line_number)? {
            trap_expectations.push(trap_exp);
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            seen_glsl_code = true;
        }
    }

    let mut error_expectations = Vec::new();
    if test_types.contains(&TestType::Error) {
        for (line_num, line) in lines.iter().enumerate() {
            let exp = parse_expected_error::parse_expected_errors_from_line(line, line_num + 1)?;
            error_expectations.extend(exp);
        }
    }

    let (glsl_source, clif_expectations) =
        parse_source::extract_source_and_expectations(&lines, &test_types)?;

    Ok(TestFile {
        glsl_source,
        run_directives,
        trap_expectations,
        test_types,
        annotations: file_annotations,
        clif_expectations,
        error_expectations,
    })
}

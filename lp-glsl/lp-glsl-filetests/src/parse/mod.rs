//! Test file parsing.

pub mod parse_run;
pub mod parse_source;
pub mod parse_target;
pub mod parse_test_type;
pub mod parse_trap;
pub mod test_type;

// Re-exports
pub use test_type::{
    ClifExpectations, ComparisonOp, RunDirective, TestFile, TestType, TrapExpectation,
};

use anyhow::{Context, Result};
use std::path::Path;

/// Parse a test file and extract all directives and source code.
pub fn parse_test_file(path: &Path) -> Result<TestFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let mut test_types = Vec::new();
    let mut run_directives = Vec::new();
    let mut trap_expectations = Vec::new();
    let mut target = None;
    let mut is_test_run = false;

    // First pass: collect directives
    for (line_num, line) in lines.iter().enumerate() {
        if let Some(test_type) = parse_test_type::parse_test_type(line) {
            test_types.push(test_type);
            if matches!(test_type, TestType::Run) {
                is_test_run = true;
            }
            continue;
        }

        if let Some(t) = parse_target::parse_target_directive(line) {
            target = Some(t);
            continue;
        }

        if let Some(run_line) = parse_run::parse_run_directive_line(line) {
            let directive = parse_run::parse_run_directive(run_line, line_num + 1)?;
            run_directives.push(directive);
            continue;
        }

        if let Some(trap_exp) = parse_trap::parse_trap_expectation(line, line_num + 1)? {
            trap_expectations.push(trap_exp);
            continue;
        }
    }

    // Second pass: extract GLSL source and CLIF expectations
    let (glsl_source, clif_expectations) =
        parse_source::extract_source_and_expectations(&lines, &test_types)?;

    Ok(TestFile {
        glsl_source,
        run_directives,
        trap_expectations,
        target,
        is_test_run,
        test_types,
        clif_expectations,
    })
}

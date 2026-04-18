//! Test file parsing.

pub mod parse_annotation;
pub mod parse_compile_opt;
pub mod parse_expected_error;
pub mod parse_run;
pub mod parse_set_uniform;
pub mod parse_source;
pub mod parse_target;
pub mod parse_test_type;
pub mod parse_trap;
pub mod test_type;

// Re-exports
pub use test_type::{
    ClifExpectations, ComparisonOp, ErrorExpectation, RunDirective, SetUniform, TestFile, TestType,
    TrapExpectation,
};

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

/// Strip `/* … */` segments from one line, updating cross-line block-comment state.
///
/// Filetest directives (`// run:`, `// @…`) must not be recognized when they appear only inside
/// GLSL block comments; the GLSL front end sees those lines as comments, but a naive per-line
/// `//` scan would still treat `// run:` as an active directive.
fn strip_block_comment_fragments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        if *in_block_comment {
            match rest.find("*/") {
                Some(i) => {
                    rest = &rest[i + 2..];
                    *in_block_comment = false;
                }
                None => return String::new(),
            }
        } else {
            match rest.find("/*") {
                Some(i) => {
                    out.push_str(&rest[..i]);
                    rest = &rest[i + 2..];
                    *in_block_comment = true;
                }
                None => {
                    out.push_str(rest);
                    break;
                }
            }
        }
    }
    out
}

/// Parse a test file and extract all directives and source code.
pub fn parse_test_file(path: &Path) -> Result<TestFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let mut test_types = Vec::new();
    let mut run_directives = Vec::new();
    let mut trap_expectations = Vec::new();
    let mut pending_annotations: Vec<crate::targets::Annotation> = Vec::new();
    let mut pending_set_uniforms: Vec<SetUniform> = Vec::new();
    let mut config_overrides: Vec<(String, String)> = Vec::new();
    let mut seen_compile_opt_keys: HashSet<String> = HashSet::new();

    let mut in_block_comment = false;
    for (line_num, line) in lines.iter().enumerate() {
        let line_number = line_num + 1;
        let logical = strip_block_comment_fragments(line, &mut in_block_comment);

        if let Some(test_type) = parse_test_type::parse_test_type(&logical) {
            test_types.push(test_type);
            continue;
        }

        if parse_target::parse_target_directive(&logical).is_some() {
            continue;
        }

        if let Some((key, value)) =
            parse_compile_opt::parse_compile_opt_line(&logical, line_number)?
        {
            if !seen_compile_opt_keys.insert(key.clone()) {
                anyhow::bail!("line {line_number}: duplicate `compile-opt` key {key:?}");
            }
            config_overrides.push((key, value));
            continue;
        }

        if let Ok(Some(annotation)) = parse_annotation::parse_annotation_line(&logical, line_number)
        {
            pending_annotations.push(annotation);
            continue;
        }

        if let Some(body) = parse_set_uniform::parse_set_uniform_line(&logical) {
            pending_set_uniforms.push(parse_set_uniform::parse_set_uniform_body(
                body,
                line_number,
            )?);
            continue;
        }

        if let Some(run_line) = parse_run::parse_run_directive_line(&logical) {
            let legacy_expect_fail = run_line.trim_end().ends_with("[expect-fail]");
            let mut directive =
                parse_run::parse_run_directive(run_line, line_number, legacy_expect_fail)?;
            directive.annotations = std::mem::take(&mut pending_annotations);
            directive.set_uniforms = std::mem::take(&mut pending_set_uniforms);
            run_directives.push(directive);
            continue;
        }

        if let Some(trap_exp) = parse_trap::parse_trap_expectation(&logical, line_number)? {
            trap_expectations.push(trap_exp);
            continue;
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
        clif_expectations,
        error_expectations,
        config_overrides,
    })
}

#[cfg(test)]
mod block_comment_directive_tests {
    use super::*;

    #[test]
    fn run_inside_glsl_block_comment_is_ignored() {
        let p =
            std::env::temp_dir().join(format!("lps_ft_block_comment_{}.glsl", std::process::id()));
        std::fs::write(
            &p,
            r"// test run
float f() { return 1.0; }
/*
// run: ghost() ~= 0.0
*/
// run: f() ~= 1.0
",
        )
        .unwrap();
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(tf.run_directives.len(), 1, "{:?}", tf.run_directives);
    }

    #[test]
    fn run_on_same_line_after_block_close_is_seen() {
        let p = std::env::temp_dir().join(format!(
            "lps_ft_block_same_line_{}.glsl",
            std::process::id()
        ));
        std::fs::write(
            &p,
            r"// test run
float f() { return 2.0; }
/* c */ // run: f() ~= 2.0
",
        )
        .unwrap();
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(tf.run_directives.len(), 1);
    }

    #[test]
    fn duplicate_compile_opt_key_errors() {
        let p = std::env::temp_dir().join(format!(
            "lps_ft_dup_compile_opt_{}.glsl",
            std::process::id()
        ));
        std::fs::write(
            &p,
            r"// test run
// compile-opt(inline.mode, never)
// compile-opt(inline.mode, always)
float f() { return 1.0; }
// run: f() ~= 1.0
",
        )
        .unwrap();
        let r = parse_test_file(&p);
        let _ = std::fs::remove_file(&p);
        assert!(r.is_err(), "expected duplicate key error");
    }
}

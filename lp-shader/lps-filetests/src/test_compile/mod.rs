//! Compile test implementation.

use crate::parse::TestFile;
use crate::targets::{Disposition, Target, directive_disposition};
use crate::test_run::compile::{build_compiler_config, compile_for_target};
use crate::test_run::{PerTargetStats, TestCaseStats};
use anyhow::Result;
use lp_emu_core::LogLevel;
use std::collections::BTreeMap;
use std::path::Path;

/// Run a `// test compile` file against all requested targets.
///
/// Compile tests stop after frontend/backend compilation and do not execute any exported shader
/// function. They are useful for broad language-coverage gates where numeric backend differences
/// would make `// test run` too strong.
///
/// Dispositions come from the file's annotations
/// ([`TestFile::file_annotations`]) — a compile-only file has no `// run:` line
/// to hang them off, so `@unsupported(*)` and friends are written at file level
/// and apply to the single compile "case" per target.
pub fn run_compile_test(
    test_file: &TestFile,
    path: &Path,
    targets: &[&Target],
) -> Result<(
    Result<()>,
    PerTargetStats,
    TestCaseStats,
    BTreeMap<String, bool>,
    bool,
)> {
    let compiler_config = build_compiler_config(&test_file.config_overrides)?;
    let relative_path = path.to_string_lossy();
    let mut per_target = BTreeMap::new();
    let mut combined_stats = TestCaseStats::default();
    let mut compile_failed_by_target = BTreeMap::new();
    let mut errors = Vec::new();

    for target in targets {
        let target_name = target.name();
        let mut stats = TestCaseStats {
            total: 1,
            ..TestCaseStats::default()
        };

        let disposition = directive_disposition(&test_file.file_annotations, target);
        if disposition == Disposition::Skip {
            stats.unsupported = 1;
            combined_stats.add(&stats);
            per_target.insert(target_name, stats);
            continue;
        }

        let compiled = compile_for_target(
            &test_file.glsl_source,
            target,
            &relative_path,
            LogLevel::None,
            &compiler_config,
            &test_file.texture_specs,
        );
        let compile_error = compiled.err();
        compile_failed_by_target.insert(target_name.clone(), compile_error.is_some());

        match (disposition, compile_error) {
            (Disposition::ExpectSuccess, None) => stats.passed = 1,
            (Disposition::ExpectSuccess, Some(err)) => {
                stats.failed = 1;
                errors.push(format!(
                    "{}: compile failed for {}:\n\n{err:#}",
                    path.display(),
                    target_name
                ));
            }
            (Disposition::ExpectFailure(_), Some(_)) => stats.unimplemented = 1,
            (Disposition::ExpectFailure(kind), None) => {
                stats.unexpected_pass = 1;
                errors.push(format!(
                    "{}: compiles for {} but is annotated @{} — remove the annotation",
                    path.display(),
                    target_name,
                    kind.keyword()
                ));
            }
            (Disposition::Skip, _) => unreachable!("skip returns above"),
        }

        combined_stats.add(&stats);
        per_target.insert(target_name, stats);
    }

    let any_compile_failed = compile_failed_by_target.values().any(|&failed| failed);
    let result = if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(errors.join("\n\n")))
    };

    Ok((
        result,
        per_target,
        combined_stats,
        compile_failed_by_target,
        any_compile_failed,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_test_file;

    fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("lps_ft_compile_{name}_{}.glsl", std::process::id()));
        std::fs::write(&p, contents).unwrap();
        p
    }

    /// A `// test compile` file has no `// run:` to hang annotations off, so
    /// they land at file level — this is the mechanism that makes compile-only
    /// files triageable per target at all.
    #[test]
    fn compile_file_annotations_are_file_level() {
        let p = write_temp(
            "filelevel",
            "// test compile\n// @unsupported(*)\n\nfloat f() { return 1.0; }\n",
        );
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(tf.file_annotations.len(), 1);
        assert!(tf.run_directives.is_empty());
        for target in crate::targets::ALL_TARGETS {
            assert_eq!(
                directive_disposition(&tf.file_annotations, target),
                Disposition::Skip,
                "{}",
                target.name()
            );
        }
    }

    /// A `// test run` file keeps the old behaviour: annotations attach to the
    /// next `// run:` and nothing lands at file level.
    #[test]
    fn run_file_annotations_stay_on_their_directive() {
        let p = write_temp(
            "runlevel",
            "// test run\nfloat f() { return 1.0; }\n// @unsupported(wasm.q32)\n// run: f() ~= 1.0\n",
        );
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert!(tf.file_annotations.is_empty());
        assert_eq!(tf.run_directives[0].annotations.len(), 1);
    }

    /// An unsupported target is skipped without compiling — the file counts one
    /// unsupported case for it, not a pass and not a failure.
    #[test]
    fn unsupported_compile_target_is_skipped_not_compiled() {
        let p = write_temp(
            "skip",
            "// test compile\n// @unsupported(backend=wasm)\n\nthis is not glsl at all\n",
        );
        let tf = parse_test_file(&p).unwrap();
        let wasm = crate::targets::Target::from_name("wasm.q32").unwrap();
        let (result, per_target, stats, _, any_failed) =
            run_compile_test(&tf, &p, &[wasm]).unwrap();
        let _ = std::fs::remove_file(&p);
        assert!(result.is_ok(), "skipped target must not report an error");
        assert!(!any_failed);
        assert_eq!(stats.unsupported, 1);
        assert_eq!(stats.passed, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(per_target["wasm.q32"].unsupported, 1);
    }

    /// `@unimplemented` on a file that really does not compile is an expected
    /// failure, exactly as it is for a `// run:` directive.
    #[test]
    fn unimplemented_compile_failure_is_expected() {
        let p = write_temp(
            "unimpl",
            "// test compile\n// @unimplemented(*)\n\nthis is not glsl at all\n",
        );
        let tf = parse_test_file(&p).unwrap();
        let wasm = crate::targets::Target::from_name("wasm.q32").unwrap();
        let (result, _, stats, _, _) = run_compile_test(&tf, &p, &[wasm]).unwrap();
        let _ = std::fs::remove_file(&p);
        assert!(result.is_ok());
        assert_eq!(stats.unimplemented, 1);
        assert_eq!(stats.failed, 0);
    }

    /// A stale annotation on a file that now compiles is reported, not ignored.
    #[test]
    fn unimplemented_that_compiles_is_an_unexpected_pass() {
        let p = write_temp(
            "stale",
            "// test compile\n// @unimplemented(*)\n\nfloat f() { return 1.0; }\n",
        );
        let tf = parse_test_file(&p).unwrap();
        let wasm = crate::targets::Target::from_name("wasm.q32").unwrap();
        let (result, _, stats, _, _) = run_compile_test(&tf, &p, &[wasm]).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(stats.unexpected_pass, 1);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unimplemented"), "{err}");
    }

    /// Unannotated compile files behave exactly as before.
    #[test]
    fn unannotated_compile_file_passes() {
        let p = write_temp("plain", "// test compile\n\nfloat f() { return 1.0; }\n");
        let tf = parse_test_file(&p).unwrap();
        let wasm = crate::targets::Target::from_name("wasm.q32").unwrap();
        let (result, _, stats, _, any_failed) = run_compile_test(&tf, &p, &[wasm]).unwrap();
        let _ = std::fs::remove_file(&p);
        assert!(result.is_ok());
        assert!(!any_failed);
        assert_eq!(stats.passed, 1);
    }
}

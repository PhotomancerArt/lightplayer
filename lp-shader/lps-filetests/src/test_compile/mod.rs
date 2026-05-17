//! Compile test implementation.

use crate::parse::TestFile;
use crate::targets::Target;
use crate::test_run::compile::{build_compiler_config, compile_for_target};
use crate::test_run::{PerTargetStats, TestCaseStats};
use anyhow::Result;
use lp_riscv_emu::LogLevel;
use std::collections::BTreeMap;
use std::path::Path;

/// Run a `// test compile` file against all requested targets.
///
/// Compile tests stop after frontend/backend compilation and do not execute any exported shader
/// function. They are useful for broad language-coverage gates where numeric backend differences
/// would make `// test run` too strong.
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

        match compile_for_target(
            &test_file.glsl_source,
            target,
            &relative_path,
            LogLevel::None,
            &compiler_config,
            &test_file.texture_specs,
        ) {
            Ok(_) => {
                stats.passed = 1;
                compile_failed_by_target.insert(target_name.clone(), false);
            }
            Err(err) => {
                stats.failed = 1;
                compile_failed_by_target.insert(target_name.clone(), true);
                errors.push(format!(
                    "{}: compile failed for {}:\n\n{err:#}",
                    path.display(),
                    target_name
                ));
            }
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

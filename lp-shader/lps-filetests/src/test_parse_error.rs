//! Runner for `// test parse-error` files (expect [`crate::parse::parse_test_file`] to fail).

use crate::parse::{self, parse_expect_parse_failure};
use crate::test_run::TestCaseStats;
use anyhow::{Result, anyhow};
use std::path::Path;

/// Run a parse-error filetest: compilation of harness metadata must fail with a substring match.
pub fn run_parse_error_test(
    path: &Path,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let expected =
        parse_expect_parse_failure::parse_expect_parse_failure_from_contents(&contents, path)?;

    let mut stats = TestCaseStats::default();
    stats.total = 1;

    match parse::parse_test_file(path) {
        Ok(_) => {
            stats.failed = 1;
            Ok((
                Err(anyhow!(
                    "expected parse_test_file to fail for {}",
                    path.display()
                )),
                stats,
                Vec::new(),
                vec![1],
            ))
        }
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains(expected.as_str()) {
                stats.passed = 1;
                Ok((Ok(()), stats, Vec::new(), Vec::new()))
            } else {
                stats.failed = 1;
                Ok((
                    Err(anyhow!(
                        "parse failed, but message did not contain expected substring {expected:?}.\n\nActual error:\n{msg}"
                    )),
                    stats,
                    Vec::new(),
                    vec![1],
                ))
            }
        }
    }
}

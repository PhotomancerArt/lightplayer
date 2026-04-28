//! Parse `// expect-parse-failure: {{substring}}` for `// test parse-error` files.

use anyhow::{Context, Result};
use std::path::Path;

use super::parse_expected_error::extract_brace_content;

/// Extract the single `// expect-parse-failure: {{…}}` message from a file's source text.
pub(crate) fn parse_expect_parse_failure_from_contents(
    contents: &str,
    path: &Path,
) -> Result<String> {
    let mut found: Option<String> = None;
    for (i, line) in contents.lines().enumerate() {
        let line_number = i + 1;
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("// expect-parse-failure:") else {
            continue;
        };
        let msg = extract_brace_content(rest.trim()).ok_or_else(|| {
            anyhow::anyhow!(
                "{}: line {line_number}: expect-parse-failure must use {{message}}",
                path.display()
            )
        })?;
        if found.is_some() {
            anyhow::bail!(
                "{}: duplicate // expect-parse-failure: (at line {line_number})",
                path.display()
            );
        }
        found = Some(msg);
    }
    found.with_context(|| {
        format!(
            "{}: missing `// expect-parse-failure: {{...}}`",
            path.display()
        )
    })
}

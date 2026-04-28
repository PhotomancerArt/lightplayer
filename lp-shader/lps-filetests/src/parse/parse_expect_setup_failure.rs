//! Parse `// EXPECT_SETUP_FAILURE: {{substring}}` before a `// run:` directive.

use anyhow::Result;

use super::parse_expected_error::extract_brace_content;

/// When `line` is `// EXPECT_SETUP_FAILURE: {{msg}}`, returns `Some(msg)`.
pub(crate) fn parse_expect_setup_failure_line(
    line: &str,
    line_number: usize,
) -> Result<Option<String>> {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix("// EXPECT_SETUP_FAILURE:") else {
        return Ok(None);
    };
    let msg = extract_brace_content(rest.trim()).ok_or_else(|| {
        anyhow::anyhow!("line {line_number}: EXPECT_SETUP_FAILURE must use {{message}} substring")
    })?;
    Ok(Some(msg))
}

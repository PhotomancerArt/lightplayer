use anyhow::{Context, Result};
use fw_checks::checks::shader_compile::report::report_from_jsonl;

pub fn write_report(check_slug: &str, records: &str, path: &std::path::Path) -> Result<()> {
    let report = match check_slug {
        "shader-compile-stress" => report_from_jsonl(records)
            .context("build shader compile report")?
            .unwrap_or_else(|| String::from("No structured records found.\n")),
        _ => String::from("No report generator registered for this check.\n"),
    };
    std::fs::write(path, report).with_context(|| format!("write report {}", path.display()))
}

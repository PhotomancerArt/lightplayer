use crate::stats::{ModuleStats, StatsDelta};
use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct ReportMetadata {
    pub git_hash: String,
    pub timestamp: String,
    pub test_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverallReport {
    pub metadata: ReportMetadata,
    pub summary: ModuleStatsSummary,
    pub tests: Vec<TestSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleStatsSummary {
    pub before: ModuleStats,
    pub after: ModuleStats,
    pub delta: StatsDelta,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestSummary {
    pub name: String,
    pub before: ModuleStats,
    pub after: ModuleStats,
    pub delta: StatsDelta,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestReport {
    pub name: String,
    pub before: ModuleStats,
    pub after: ModuleStats,
    pub delta: StatsDelta,
    pub functions: Vec<FunctionReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionReport {
    pub name: String,
    pub before: crate::stats::FunctionStats,
    pub after: crate::stats::FunctionStats,
    pub delta: StatsDelta,
}

pub fn collect_git_hash() -> String {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();

    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    }
}

pub fn generate_reports(report_dir: &Path, test_summaries: &[TestSummary]) -> Result<()> {
    // Calculate overall summary
    let mut total_before = ModuleStats {
        total_blocks: 0,
        total_instructions: 0,
        total_values: 0,
        total_clif_size: 0,
        total_vcode_size: 0,
        total_assembly_size: 0,
        functions: Vec::new(),
    };
    let mut total_after = ModuleStats {
        total_blocks: 0,
        total_instructions: 0,
        total_values: 0,
        total_clif_size: 0,
        total_vcode_size: 0,
        total_assembly_size: 0,
        functions: Vec::new(),
    };

    for summary in test_summaries {
        total_before.total_blocks += summary.before.total_blocks;
        total_before.total_instructions += summary.before.total_instructions;
        total_before.total_values += summary.before.total_values;
        total_before.total_clif_size += summary.before.total_clif_size;
        total_before.total_vcode_size += summary.before.total_vcode_size;
        total_before.total_assembly_size += summary.before.total_assembly_size;

        total_after.total_blocks += summary.after.total_blocks;
        total_after.total_instructions += summary.after.total_instructions;
        total_after.total_values += summary.after.total_values;
        total_after.total_clif_size += summary.after.total_clif_size;
        total_after.total_vcode_size += summary.after.total_vcode_size;
        total_after.total_assembly_size += summary.after.total_assembly_size;
    }

    let summary_delta = crate::stats::calculate_deltas(&total_before, &total_after);

    // Create metadata
    let metadata = ReportMetadata {
        git_hash: collect_git_hash(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        test_count: test_summaries.len(),
    };

    // Create overall report
    let report = OverallReport {
        metadata,
        summary: ModuleStatsSummary {
            before: total_before,
            after: total_after,
            delta: summary_delta,
        },
        tests: test_summaries.to_vec(),
    };

    // Serialize to TOML
    let toml = toml::to_string_pretty(&report)
        .map_err(|e| anyhow::anyhow!("Failed to serialize report: {}", e))?;

    // Write report.toml
    let report_file = report_dir.join("report.toml");
    fs::write(&report_file, toml).map_err(|e| anyhow::anyhow!("Failed to write report: {}", e))?;

    Ok(())
}

pub fn generate_test_report(test_dir: &Path, test_report: &TestReport) -> Result<()> {
    // Serialize to TOML
    let toml = toml::to_string_pretty(test_report)
        .map_err(|e| anyhow::anyhow!("Failed to serialize test report: {}", e))?;

    // Write stats.toml
    let stats_file = test_dir.join("stats.toml");
    fs::write(&stats_file, toml)
        .map_err(|e| anyhow::anyhow!("Failed to write test report: {}", e))?;

    Ok(())
}

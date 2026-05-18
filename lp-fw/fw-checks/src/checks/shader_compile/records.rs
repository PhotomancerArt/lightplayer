use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ShaderCompileRecord {
    CaseSummary(ShaderCompileCaseSummary),
    TotalSummary(ShaderCompileTotalSummary),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShaderCompileCaseSummary {
    pub check: String,
    pub case: String,
    pub build_us: u64,
    pub ticks: u32,
    pub max_slice_us: u64,
    #[serde(default)]
    pub max_slice_stage: String,
    pub peak_used: usize,
    pub resident_used: usize,
    pub after_drop_used: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShaderCompileTotalSummary {
    pub check: String,
    pub build_us: u64,
    pub cases: usize,
    pub worst_slice_us: u64,
    pub worst_peak_used: usize,
}

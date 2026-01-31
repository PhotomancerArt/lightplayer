use clap::Parser;
use lp_glsl_compiler::backend::transform::q32::FixedPointFormat;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lp-glsl-q32-metrics-app")]
#[command(about = "Track q32 transform bloat metrics")]
pub struct Args {
    /// Directory containing GLSL test files
    #[arg(long, required = true)]
    pub tests_dir: PathBuf,

    /// Directory for report output
    #[arg(long, required = true)]
    pub output_dir: PathBuf,

    /// Report name (appended to timestamp in directory name)
    #[arg(required = true)]
    pub report_name: String,

    /// Fixed point format
    #[arg(long, default_value = "Fixed16x16")]
    pub format: String,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

pub fn parse_args() -> Args {
    Args::parse()
}

pub fn parse_format(format_str: &str) -> anyhow::Result<FixedPointFormat> {
    match format_str {
        "Fixed16x16" => Ok(FixedPointFormat::Fixed16x16),
        "Q32x32" => Ok(FixedPointFormat::Q32x32),
        _ => anyhow::bail!(
            "Unknown format: {}. Supported: Fixed16x16, Q32x32",
            format_str
        ),
    }
}

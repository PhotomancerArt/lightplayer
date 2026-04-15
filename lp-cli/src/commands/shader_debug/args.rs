//! Arguments for `shader-debug`.

use std::path::PathBuf;

use clap::Parser;

use super::types::{BackendTarget, SectionFilter};

#[derive(Debug, Parser)]
#[command(about = "Unified shader debug output for all backends")]
pub struct Args {
    /// Path to GLSL source
    pub input: PathBuf,

    /// Comma-separated list of targets (rv32n, rv32c, emu)
    #[arg(short, long, default_value = "rv32n")]
    pub target: String,

    /// Function name to filter (shows all by default)
    #[arg(id = "fn", long = "fn", value_name = "NAME")]
    pub func: Option<String>,

    /// Floating point mode
    #[arg(long, default_value = "q32", value_name = "MODE")]
    pub float_mode: String,

    /// Show LPIR section
    #[arg(long)]
    pub lpir: bool,

    /// Show VInst/interleaved section
    #[arg(long)]
    pub vinst: bool,

    /// Show assembly/disasm section
    #[arg(long)]
    pub asm: bool,

    /// Summary only - don't show detailed function output
    #[arg(long)]
    pub summary: bool,
}

impl Args {
    /// Parse the targets string into a list of BackendTarget.
    ///
    /// Supports comma-separated targets like "rv32c,rv32n"
    pub fn targets(&self) -> Vec<BackendTarget> {
        self.target
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<BackendTarget>())
            .filter_map(Result::ok)
            .collect()
    }

    /// Determine which sections to show.
    ///
    /// Default behavior (no flags): show all sections
    /// With explicit flags: show only selected sections
    pub fn sections(&self) -> SectionFilter {
        let any_explicit = self.lpir || self.vinst || self.asm;

        if any_explicit {
            SectionFilter {
                lpir: self.lpir,
                vinst: self.vinst,
                asm: self.asm,
            }
        } else {
            SectionFilter::all()
        }
    }
}

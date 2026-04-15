//! Arguments for `shader-rv32n`.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use super::pipeline::Verbosity;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ArtifactFormat {
    /// Annotated assembly (`.globl` + PInst text per function).
    Text,
    /// Raw RISC-V machine code (little-endian u32 per insn, functions concatenated).
    Bin,
    /// One `xxxxxxxx` hex line per 32-bit word (concatenated functions).
    Hex,
}

#[derive(Debug, Parser)]
#[command(about = "Fastalloc RV32 pipeline: GLSL → LPIR → VInst → PInst → machine code")]
pub struct Args {
    /// Path to GLSL source
    pub input: PathBuf,

    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    #[arg(long, default_value = "q32", value_name = "MODE")]
    pub float_mode: String,

    /// Final artifact format on stdout or `-o` (debug listings stay on stderr).
    #[arg(long, value_enum, default_value_t = ArtifactFormat::Text)]
    pub format: ArtifactFormat,

    /// Hide LPIR dump on stderr (verbose by default).
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_lpir: bool,

    /// Hide VInst listing on stderr.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_vinst: bool,

    /// Hide PInst listing on stderr.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_pinst: bool,

    /// Hide per-instruction disassembly on stderr.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_disasm: bool,

    /// Show region tree structure on stderr.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub show_region: bool,

    /// Show liveness analysis on stderr.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub show_liveness: bool,

    /// Hide all stderr listings (same as every `--no-*` above).
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub quiet: bool,
}

impl Args {
    pub fn verbosity(&self) -> Verbosity {
        let q = self.quiet;
        Verbosity {
            vinst: !q && !self.no_vinst,
            pinst: !q && !self.no_pinst,
            disasm: !q && !self.no_disasm,
            region: self.show_region,
            liveness: self.show_liveness,
        }
    }

    pub fn show_lpir(&self) -> bool {
        !self.quiet && !self.no_lpir
    }
}

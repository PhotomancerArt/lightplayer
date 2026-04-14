//! Arguments for `shader-debug`.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Backend target for debug output.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum BackendTarget {
    /// Fastalloc-inspired allocator (FA pipeline).
    Rv32fa,
    /// Linear scan allocator (native pipeline).
    Rv32lp,
    /// Cranelift-based JIT.
    Rv32,
    /// Cranelift-based emulator.
    Emu,
}

impl BackendTarget {
    /// Returns the target name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendTarget::Rv32fa => "rv32fa",
            BackendTarget::Rv32lp => "rv32lp",
            BackendTarget::Rv32 => "rv32",
            BackendTarget::Emu => "emu",
        }
    }
}

#[derive(Debug, Parser)]
#[command(about = "Unified shader debug output for all backends")]
pub struct Args {
    /// Path to GLSL source
    pub input: PathBuf,

    /// Backend target
    #[arg(short, long, value_enum, default_value_t = BackendTarget::Rv32fa)]
    pub target: BackendTarget,

    /// Function name to filter (shows all by default)
    #[arg(id = "fn", long = "fn", value_name = "NAME")]
    pub func: Option<String>,

    /// Floating point mode
    #[arg(long, default_value = "q32", value_name = "MODE")]
    pub float_mode: String,
}

//! Arguments for `shader-rv32`.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ShaderRv32Args {
    pub path: PathBuf,
    pub output: Option<PathBuf>,
    pub float_mode: String,
    pub hex: bool,
    /// Print register allocation trace to stderr (linear scan / liveness).
    pub alloc_trace: bool,
}

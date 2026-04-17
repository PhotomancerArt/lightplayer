//! Unified debug data structures for shader-debug command.

/// Debug info for a single function from a single backend.
pub struct FunctionDebugData {
    pub name: String,
    pub lpir_count: usize,
    /// `weight_body_len` from lpir inline_weights when `--weights` is used; otherwise 0.
    pub weight_body_len: usize,
    /// Markers-zero weight (`mz` column).
    pub weight_mz: usize,
    /// Heavy-bias weight (`hb` column).
    pub weight_hb: usize,
    pub disasm_count: usize,
    pub spill_slots: Option<usize>,  // FA only
    pub interleaved: Option<String>, // FA only
    pub disasm: String,
    pub has_vinst: bool, // true for FA
}

impl FunctionDebugData {
    pub fn new(name: String) -> Self {
        Self {
            name,
            lpir_count: 0,
            weight_body_len: 0,
            weight_mz: 0,
            weight_hb: 0,
            disasm_count: 0,
            spill_slots: None,
            interleaved: None,
            disasm: String::new(),
            has_vinst: false,
        }
    }
}

/// Debug data for all functions from a single backend.
pub struct BackendDebugData {
    pub backend: String, // "rv32c", "rv32n", "emu"
    pub functions: Vec<FunctionDebugData>,
}

impl BackendDebugData {
    pub fn new(backend: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            functions: Vec::new(),
        }
    }

    /// Get function data by name.
    pub fn get_function(&self, name: &str) -> Option<&FunctionDebugData> {
        self.functions.iter().find(|f| f.name == name)
    }
}

/// Complete debug data from all backends.
pub struct DebugReport {
    pub backends: Vec<BackendDebugData>,
}

impl DebugReport {
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Get all unique function names across all backends.
    pub fn function_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .backends
            .iter()
            .flat_map(|b| b.functions.iter().map(|f| f.name.as_str()))
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// Section selection flags.
pub struct SectionFilter {
    pub lpir: bool,
    pub vinst: bool,
    pub asm: bool,
}

impl SectionFilter {
    /// Default: show all sections.
    pub fn all() -> Self {
        Self {
            lpir: true,
            vinst: true,
            asm: true,
        }
    }
}

/// Target specification for parsing CLI arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendTarget {
    Rv32fa,
    Rv32,
    Emu,
}

impl std::str::FromStr for BackendTarget {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rv32n" => Ok(BackendTarget::Rv32fa),
            "rv32c" => Ok(BackendTarget::Rv32),
            "emu" => Ok(BackendTarget::Emu),
            _ => Err(format!("unknown target: {s}")),
        }
    }
}

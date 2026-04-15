//! Unified debug data structures for shader-debug command.

use std::collections::BTreeMap;

/// Debug info for a single function from a single backend.
pub struct FunctionDebugData {
    pub name: String,
    pub lpir_count: usize,
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

    /// Get instruction counts for all functions as a map.
    pub fn inst_counts(&self) -> BTreeMap<&str, usize> {
        self.functions
            .iter()
            .map(|f| (f.name.as_str(), f.disasm_count))
            .collect()
    }
}

/// Complete debug data from all backends.
pub struct DebugReport {
    pub file_path: String,
    pub backends: Vec<BackendDebugData>,
}

impl DebugReport {
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
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

    /// Get disasm count for a function from a specific backend.
    pub fn get_disasm_count(&self, func_name: &str, backend: &str) -> Option<usize> {
        self.backends
            .iter()
            .find(|b| b.backend == backend)
            .and_then(|b| b.get_function(func_name))
            .map(|f| f.disasm_count)
    }

    /// Get the minimum disasm count across all backends for a function.
    pub fn min_disasm_count(&self, func_name: &str) -> Option<usize> {
        self.backends
            .iter()
            .filter_map(|b| b.get_function(func_name).map(|f| f.disasm_count))
            .min()
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

    /// Show no sections (useful for summary-only mode).
    pub fn none() -> Self {
        Self {
            lpir: false,
            vinst: false,
            asm: false,
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

impl BackendTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendTarget::Rv32fa => "rv32n",
            BackendTarget::Rv32 => "rv32c",
            BackendTarget::Emu => "emu",
        }
    }
}

impl std::str::FromStr for BackendTarget {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rv32n" => Ok(BackendTarget::Rv32fa),
            "rv32c" => Ok(BackendTarget::Rv32),
            "emu" => Ok(BackendTarget::Emu),
            _ => Err(format!("unknown target: {}", s)),
        }
    }
}

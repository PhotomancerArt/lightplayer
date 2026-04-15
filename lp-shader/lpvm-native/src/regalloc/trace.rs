//! AllocTrace system for debugging allocator decisions.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub struct AllocTrace {
    pub entries: Vec<TraceEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceEntry {
    pub vinst_idx: usize,
    pub vinst_mnemonic: String,
    pub decision: String,
    pub register_state: String,
}

impl AllocTrace {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Reverse entries (allocator walks backward, display goes forward).
    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Format as human-readable table.
    pub fn format(&self) -> String {
        let mut lines = vec![
            "=== AllocTrace ===".into(),
            format!(
                "{:>4} | {:>10} | {:>20} | {}",
                "Idx", "VInst", "Decision", "State"
            ),
            format!("{:->4}-+-{:->10}-+-{:->20}-+-{:->20}", "", "", "", ""),
        ];

        for entry in &self.entries {
            lines.push(format!(
                "{:4} | {:>10} | {:>20} | {}",
                entry.vinst_idx, entry.vinst_mnemonic, entry.decision, entry.register_state,
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_push_and_len() {
        let mut trace = AllocTrace::new();
        assert!(trace.is_empty());

        trace.push(TraceEntry {
            vinst_idx: 0,
            vinst_mnemonic: String::from("Add"),
            decision: String::from("alloc v2"),
            register_state: String::from("v2→a0"),
        });
        assert_eq!(trace.entries.len(), 1);
    }

    #[test]
    fn trace_reverse() {
        let mut trace = AllocTrace::new();
        trace.push(TraceEntry {
            vinst_idx: 0,
            vinst_mnemonic: String::from("IConst32"),
            decision: String::from("def v0"),
            register_state: String::from("v0→a0"),
        });
        trace.push(TraceEntry {
            vinst_idx: 1,
            vinst_mnemonic: String::from("Ret"),
            decision: String::from("use v0"),
            register_state: String::from("v0→a0"),
        });

        trace.reverse();
        assert_eq!(trace.entries[0].vinst_idx, 1);
        assert_eq!(trace.entries[1].vinst_idx, 0);
    }

    #[test]
    fn trace_format_contains_header() {
        let mut trace = AllocTrace::new();
        trace.push(TraceEntry {
            vinst_idx: 0,
            vinst_mnemonic: String::from("Add"),
            decision: String::from("alloc"),
            register_state: String::from("v2→a0"),
        });

        let output = trace.format();
        assert!(output.contains("=== AllocTrace ==="));
        assert!(output.contains("Add"));
        assert!(output.contains("alloc"));
    }
}

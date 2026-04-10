# Phase 4: Trace System

## Scope

Implement AllocTrace structure for recording allocator decisions.

## Implementation

### 1. Implement trace in `trace.rs`

```rust
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::vinst::VInst;

#[derive(Debug, Clone)]
pub struct AllocTrace {
    pub entries: Vec<TraceEntry>,
}

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub vinst_idx: usize,
    pub vinst_mnemonic: String,
    pub decision: String,
    pub register_state: String,
}

impl AllocTrace {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn push(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Reverse entries (allocator walks backward, display goes forward).
    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    /// Format as human-readable table.
    pub fn format(&self) -> String {
        let mut lines = vec![
            "=== AllocTrace ===".to_string(),
            "Idx | VInst | Decision | Register State".to_string(),
            "----|-------|----------|---------------".to_string(),
        ];
        
        for entry in &self.entries {
            lines.push(format!(
                "{:3} | {:5} | {:8} | {}",
                entry.vinst_idx,
                entry.vinst_mnemonic,
                entry.decision,
                entry.register_state
            ));
        }
        
        lines.join("\n")
    }
}

/// Create a trace entry for a stubbed decision.
pub fn stub_entry(vinst_idx: usize, vinst: &VInst, message: &str) -> TraceEntry {
    TraceEntry {
        vinst_idx,
        vinst_mnemonic: vinst.mnemonic().to_string(),
        decision: message.to_string(),
        register_state: "(stub)".to_string(),
    }
}
```

### 2. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::{VInst, VReg};
    
    #[test]
    fn test_trace_basic() {
        let mut trace = AllocTrace::new();
        
        let entry = TraceEntry {
            vinst_idx: 0,
            vinst_mnemonic: "Add32".to_string(),
            decision: "STUB: assign v0->a0".to_string(),
            register_state: "a0=v0".to_string(),
        };
        
        trace.push(entry);
        assert_eq!(trace.entries.len(), 1);
    }
    
    #[test]
    fn test_trace_reverse() {
        let mut trace = AllocTrace::new();
        trace.push(TraceEntry { vinst_idx: 0, ..Default::default() });
        trace.push(TraceEntry { vinst_idx: 1, ..Default::default() });
        
        trace.reverse();
        assert_eq!(trace.entries[0].vinst_idx, 1);
        assert_eq!(trace.entries[1].vinst_idx, 0);
    }
    
    #[test]
    fn test_trace_format() {
        let mut trace = AllocTrace::new();
        trace.push(TraceEntry {
            vinst_idx: 0,
            vinst_mnemonic: "Add32".to_string(),
            decision: "STUB".to_string(),
            register_state: "(stub)".to_string(),
        });
        
        let output = trace.format();
        assert!(output.contains("=== AllocTrace ==="));
        assert!(output.contains("Add32"));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc::trace
```

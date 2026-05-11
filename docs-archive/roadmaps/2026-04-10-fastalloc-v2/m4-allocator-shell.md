# M4: Allocator Shell with CFG

## Scope of Work

Build the allocator structure: CFG construction, liveness analysis, trace system, and backward walk shell. The allocator produces PhysInsts with stubbed-out decisions (no real allocation yet).

## Files

```
lp-shader/lpvm-native/src/isa/rv32fa/
└── alloc/
    ├── mod.rs                 # NEW: FastAlloc main entry
    ├── cfg.rs                 # NEW: CFG construction and display
    ├── liveness.rs            # NEW: Liveness analysis and display
    ├── walk.rs                # NEW: Backward walk shell
    ├── spill.rs               # NEW: Spill slot management (stub)
    └── trace.rs               # NEW: AllocTrace system
```

## Implementation Details

### 1. CFG in `cfg.rs`

```rust
//! Control Flow Graph for VInst sequences.
//!
//! Even straight-line code has a CFG (single block) for consistency.

use alloc::vec::Vec;
use alloc::format;
use crate::vinst::{VInst, LabelId};

/// A basic block of VInsts.
#[derive(Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub start: usize,        // Start index in original VInst sequence
    pub end: usize,          // End index (exclusive)
    pub vinsts: Vec<VInst>,  // Copy of VInsts in this block
    pub preds: Vec<BlockId>,
    pub succs: Vec<BlockId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId(pub usize);

/// Control Flow Graph.
#[derive(Debug)]
pub struct CFG {
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

/// Build CFG from VInsts.
pub fn build_cfg(vinsts: &[VInst]) -> CFG {
    // For straight-line code: single block containing all VInsts
    // For control flow: split at Label, Br, BrIf
    // M2: Just create single block for now
    let block = BasicBlock {
        id: BlockId(0),
        start: 0,
        end: vinsts.len(),
        vinsts: vinsts.to_vec(),
        preds: vec![],
        succs: vec![],
    };

    CFG {
        blocks: vec![block],
        entry: BlockId(0),
    }
}

impl CFG {
    /// Check if this CFG contains any control flow.
    pub fn has_control_flow(&self) -> bool {
        // True if there's more than one block, or any Br/BrIf in the single block
        self.blocks.len() > 1 ||
        self.blocks[0].vinsts.iter().any(|v| matches!(v, VInst::Br { .. } | VInst::BrIf { .. }))
    }
}

/// Display CFG for debug output.
pub fn format_cfg(cfg: &CFG) -> String {
    let mut lines = vec!["=== CFG ===".to_string()];
    for block in &cfg.blocks {
        lines.push(format!("Block {}: [VInst {}..{}]", block.id.0, block.start, block.end));
        for (i, vinst) in block.vinsts.iter().enumerate() {
            lines.push(format!("  {}: {:?}", block.start + i, vinst.mnemonic()));
        }
        if !block.preds.is_empty() {
            let preds: Vec<_> = block.preds.iter().map(|b| b.0.to_string()).collect();
            lines.push(format!("  preds: [{}]", preds.join(", ")));
        }
        if !block.succs.is_empty() {
            let succs: Vec<_> = block.succs.iter().map(|b| b.0.to_string()).collect();
            lines.push(format!("  succs: [{}]", succs.join(", ")));
        }
    }
    lines.join("\n")
}
```

### 2. Liveness in `liveness.rs`

```rust
//! Liveness analysis on CFG.
//!
//! For M2 (straight-line), this is simple backward scan.
//! For control flow, would need fixed-point iteration.

use alloc::vec::Vec;
use alloc::collections::BTreeSet;
use crate::vinst::VReg;
use crate::isa::rv32fa::alloc::cfg::{CFG, BlockId};

/// Liveness information for a block.
#[derive(Debug)]
pub struct BlockLiveness {
    pub block: BlockId,
    pub live_in: BTreeSet<VReg>,
    pub live_out: BTreeSet<VReg>,
    pub live_through: Vec<BTreeSet<VReg>>,  // Per-instruction liveness
}

/// Compute liveness for CFG.
pub fn compute_liveness(cfg: &CFG) -> Vec<BlockLiveness> {
    // For straight-line single block: backward scan
    let block = &cfg.blocks[0];
    let mut live = BTreeSet::new();
    let mut per_inst: Vec<BTreeSet<VReg>> = Vec::new();

    for (idx, vinst) in block.vinsts.iter().enumerate().rev() {
        // Kill defs
        for def in vinst.defs() {
            live.remove(&def);
        }

        // Add uses
        for use_ in vinst.uses() {
            live.insert(use_);
        }

        per_inst.push(live.clone());
    }

    per_inst.reverse();

    vec![BlockLiveness {
        block: BlockId(0),
        live_in: per_inst[0].clone(),
        live_out: live.clone(),
        live_through: per_inst,
    }]
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &[BlockLiveness]) -> String {
    let mut lines = vec!["=== Liveness ===".to_string()];
    for block in liveness {
        lines.push(format!("Block {}:", block.block.0));

        let live_in: Vec<_> = block.live_in.iter().map(|v| format!("v{}", v.0)).collect();
        lines.push(format!("  live_in: {{{}}}", live_in.join(", ")));

        for (i, live) in block.live_through.iter().enumerate() {
            let live_list: Vec<_> = live.iter().map(|v| format!("v{}", v.0)).collect();
            lines.push(format!("  inst {}: {{{}}}", i, live_list.join(", ")));
        }

        let live_out: Vec<_> = block.live_out.iter().map(|v| format!("v{}", v.0)).collect();
        lines.push(format!("  live_out: {{{}}}", live_out.join(", ")));
    }
    lines.join("\n")
}
```

### 3. Trace in `trace.rs`

```rust
//! Structured trace for debugging allocator decisions.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::vinst::VInst;
use crate::isa::rv32fa::inst::PhysInst;

pub struct AllocTrace {
    entries: Vec<TraceEntry>,
}

pub struct TraceEntry {
    pub vinst_idx: usize,
    pub lpir_idx: Option<u32>,
    pub vinst: VInst,
    pub decision: String,
    pub physinsts: Vec<PhysInst>,
}

impl AllocTrace {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn push(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Reverse entries to forward order (allocator walks backward).
    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    /// Format as human-readable table.
    pub fn format_table(&self, func_name: &str) -> String {
        use crate::isa::rv32fa::debug::physinst::format_physinsts;

        let mut lines = vec![format!("=== fastalloc: {} ===", func_name)];
        lines.push(String::from(""));
        lines.push(format!(
            "{:>5} | {:>5} | {:<25} | {:<30} | {}",
            "VInst", "LPIR", "Instruction", "Decision", "PhysInst(s)"
        ));
        lines.push(format!(
            "{}",
            "-".repeat(80)
        ));

        for entry in &self.entries {
            let vinst_str = format!("{:?}", entry.vinst);
            let decision = &entry.decision;
            let phys_str = if entry.physinsts.is_empty() {
                "(no output)".to_string()
            } else {
                entry.physinsts.iter()
                    .map(|p| format!("{:?}", p.mnemonic()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            lines.push(format!(
                "{:>5} | {:>5} | {:<25} | {:<30} | {}",
                entry.vinst_idx,
                entry.lpir_idx.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string()),
                &vinst_str[..vinst_str.len().min(25)],
                &decision[..decision.len().min(30)],
                phys_str,
            ));
        }

        lines.join("\n")
    }
}

impl TraceEntry {
    pub fn new(
        vinst_idx: usize,
        lpir_idx: Option<u32>,
        vinst: VInst,
        decision: String,
        physinsts: Vec<PhysInst>,
    ) -> Self {
        Self { vinst_idx, lpir_idx, vinst, decision, physinsts }
    }
}
```

### 4. Walk Shell in `walk.rs`

```rust
//! Backward walk allocator (stub - decisions are TODO).

use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use crate::error::NativeError;
use crate::vinst::{VInst, VReg};
use crate::abi::FuncAbi;
use crate::isa::rv32fa::inst::PhysInst;
use crate::isa::rv32fa::alloc::trace::{AllocTrace, TraceEntry};
use crate::isa::rv32fa::alloc::cfg::CFG;
use crate::isa::rv32fa::alloc::liveness::compute_liveness;

pub struct WalkResult {
    pub physinsts: Vec<PhysInst>,
    pub trace: AllocTrace,
}

pub struct WalkState {
    trace: AllocTrace,
    stub_counter: usize,
}

impl WalkState {
    pub fn new() -> Self {
        Self {
            trace: AllocTrace::new(),
            stub_counter: 0,
        }
    }

    pub fn walk_backward(
        &mut self,
        cfg: &CFG,
    ) -> Result<(), NativeError> {
        let block = &cfg.blocks[0];  // Single block for M2

        for (pos, vinst) in block.vinsts.iter().enumerate().rev() {
            let entry = self.process_stub(pos, vinst)?;
            self.trace.push(entry);
        }

        Ok(())
    }

    fn process_stub(&mut self, pos: usize, vinst: &VInst) -> Result<TraceEntry, NativeError> {
        let decision = format!("TODO: allocate for {:?}", vinst.mnemonic());
        let physinsts = vec![];  // Empty for now

        Ok(TraceEntry::new(
            pos,
            vinst.src_op(),
            vinst.clone(),
            decision,
            physinsts,
        ))
    }

    pub fn finish(mut self) -> AllocTrace {
        self.trace.reverse();
        self.trace
    }
}
```

### 5. Main Entry in `mod.rs`

```rust
//! Fast register allocator for RV32.

pub mod cfg;
pub mod liveness;
pub mod spill;
pub mod trace;
pub mod walk;

use alloc::vec::Vec;
use crate::error::NativeError;
use crate::vinst::VInst;
use crate::abi::FuncAbi;
use crate::isa::rv32fa::inst::PhysInst;
use crate::isa::rv32fa::alloc::{
    cfg::{build_cfg, CFG},
    liveness::compute_liveness,
    walk::{WalkResult, WalkState},
};

pub struct AllocResult {
    pub physinsts: Vec<PhysInst>,
    pub trace: trace::AllocTrace,
    pub cfg: CFG,
}

pub struct FastAlloc;

impl FastAlloc {
    pub fn allocate(
        vinsts: &[VInst],
        num_vregs: usize,
        abi: &FuncAbi,
    ) -> Result<AllocResult, NativeError> {
        // Build CFG
        let cfg = build_cfg(vinsts);

        // Check for control flow (M2 limitation)
        if cfg.has_control_flow() {
            return Err(NativeError::FastallocUnsupportedControlFlow {
                ir_function_name: abi.name.clone(),
                message: "Fast allocator M2 only supports straight-line code".into(),
                trace: None,
            });
        }

        // Compute liveness (for debug display)
        let _liveness = compute_liveness(&cfg);

        // Walk and allocate (stubbed)
        let mut state = WalkState::new();
        state.walk_backward(&cfg)?;
        let trace = state.finish();

        // Build function (empty for now)
        let physinsts = vec![];

        Ok(AllocResult { physinsts, trace, cfg })
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::vinst::parse_vinsts;

    #[test]
    fn test_build_cfg() {
        let vinsts = parse_vinsts("v0 = Add32 v1, v2").unwrap();
        let cfg = build_cfg(&vinsts);
        assert_eq!(cfg.blocks.len(), 1);
        assert_eq!(cfg.blocks[0].vinsts.len(), 1);
    }

    #[test]
    fn test_rejects_control_flow() {
        let vinsts = parse_vinsts("Br 0").unwrap();
        let abi = test_abi();
        let err = FastAlloc::allocate(&vinsts, 0, &abi).unwrap_err();
        assert!(matches!(err, NativeError::FastallocUnsupportedControlFlow { .. }));
    }

    #[test]
    fn test_produces_trace() {
        let vinsts = parse_vinsts("
            v0 = IConst32 1
            v1 = Add32 v0, v0
        ").unwrap();
        let abi = test_abi();
        let result = FastAlloc::allocate(&vinsts, 2, &abi).unwrap();

        assert!(!result.trace.entries.is_empty());

        let table = result.trace.format_table("test");
        assert!(table.contains("fastalloc: test"));
        assert!(table.contains("TODO"));  // Stub decisions
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- rv32fa::alloc

# Try the CLI (should show trace with TODOs)
cargo run -p lp-cli -- shader-rv32fa test.glsl --show-cfg --show-liveness --trace
```

## Success Criteria

1. CFG is built and can be displayed with `--show-cfg`
2. Liveness is computed and can be displayed with `--show-liveness`
3. Trace is populated with entries for each VInst
4. Trace shows "TODO" decisions (stubbed)
5. Control flow is detected and rejected with error
6. CLI shows all debug output stages

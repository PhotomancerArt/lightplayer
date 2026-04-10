# Phase 1: Create alloc/ Module Structure

## Scope

Create the `alloc/` directory and module files with placeholder implementations.

## Implementation

### 1. Create directory

```bash
mkdir -p lp-shader/lpvm-native/src/isa/rv32fa/alloc
```

### 2. Create `alloc/mod.rs`

```rust
//! Fast allocator shell - CFG, liveness, trace, backward walk.

pub mod cfg;
pub mod liveness;
pub mod trace;
pub mod walk;

use alloc::vec::Vec;
use lpir::{IrFunction, VReg};
use crate::abi::FuncAbi;
use crate::vinst::VInst;
use crate::isa::rv32fa::inst::PInst;
use crate::error::NativeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    UnsupportedControlFlow,
    // ... other errors
}

/// FastAlloc shell - builds CFG, liveness, trace, walks backward with stubs.
pub struct FastAlloc;

impl FastAlloc {
    pub fn allocate(
        vinsts: &[VInst],
        func_abi: &FuncAbi,
        func: &IrFunction,
    ) -> Result<(Vec<PInst>, trace::AllocTrace), AllocError> {
        // TODO(Phase 2-5): Build CFG, liveness, walk, return trace
        // For now: return empty trace and use simple allocation
        let trace = trace::AllocTrace::new();
        let physinsts = simple_allocate(vinsts, func_abi, func)?;
        Ok((physinsts, trace))
    }
}

// Temporary: simple allocation until walk.rs is implemented
fn simple_allocate(
    vinsts: &[VInst],
    func_abi: &FuncAbi,
    func: &IrFunction,
) -> Result<Vec<PInst>, AllocError> {
    // Copy from current alloc.rs
    todo!()
}
```

### 3. Create `alloc/cfg.rs`

```rust
//! Control Flow Graph for VInst sequences.

use alloc::vec::Vec;
use crate::vinst::{VInst, LabelId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId(pub usize);

#[derive(Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub start: usize,
    pub end: usize,
    pub vinsts: Vec<VInst>,
    pub preds: Vec<BlockId>,
    pub succs: Vec<BlockId>,
}

#[derive(Debug)]
pub struct CFG {
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

/// Build CFG from VInsts.
/// M4: Single block for straight-line code.
pub fn build_cfg(vinsts: &[VInst]) -> CFG {
    todo!()
}

/// Format CFG for debug output.
pub fn format_cfg(cfg: &CFG) -> String {
    todo!()
}
```

### 4. Create `alloc/liveness.rs`

```rust
//! Liveness analysis.

use alloc::vec::Vec;
use crate::isa::rv32fa::alloc::cfg::{CFG, BlockId};
use crate::vinst::VReg;

#[derive(Debug)]
pub struct Liveness {
    pub live_in: Vec<Vec<VReg>>,   // per block
    pub live_out: Vec<Vec<VReg>>, // per block
}

/// Analyze liveness per block.
pub fn analyze_liveness(cfg: &CFG, num_vregs: usize) -> Liveness {
    todo!()
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &Liveness) -> String {
    todo!()
}
```

### 5. Create `alloc/trace.rs`

```rust
//! AllocTrace system for debugging allocator decisions.

use alloc::string::String;
use alloc::vec::Vec;
use crate::vinst::VInst;
use crate::isa::rv32fa::inst::PInst;

#[derive(Debug, Clone)]
pub struct AllocTrace {
    entries: Vec<TraceEntry>,
}

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub vinst_idx: usize,
    pub vinst: VInst,
    pub decision: TraceDecision,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum TraceDecision {
    StubAssign { vreg: u32, preg: u8 },
    StubSpill { vreg: u32, slot: u32 },
    StubReload { vreg: u32, preg: u8 },
    StubFree { preg: u8 },
    StubCall { callee: String },
}

impl AllocTrace {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn push(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Reverse entries (allocator walks backward, display forward).
    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    /// Format as human-readable table.
    pub fn format(&self) -> String {
        todo!()
    }
}
```

### 6. Create `alloc/walk.rs`

```rust
//! Backward walk allocator shell with stubbed decisions.

use alloc::vec::Vec;
use crate::isa::rv32fa::alloc::cfg::BasicBlock;
use crate::isa::rv32fa::alloc::trace::{AllocTrace, TraceEntry, TraceDecision};
use crate::vinst::VInst;

/// Walk a block backward, recording stubbed decisions.
pub fn walk_block_stub(block: &BasicBlock, trace: &mut AllocTrace) {
    for (pos, vinst) in block.vinsts.iter().enumerate().rev() {
        let entry = stub_process_instruction(pos, vinst);
        trace.push(entry);
    }
}

fn stub_process_instruction(pos: usize, vinst: &VInst) -> TraceEntry {
    // STUB: Log what we would do, don't actually do it
    let decision = TraceDecision::StubAssign { vreg: 0, preg: 0 };
    let message = format!("Would process {:?} at {}", vinst.mnemonic(), pos);
    
    TraceEntry {
        vinst_idx: pos,
        vinst: vinst.clone(),
        decision,
        message,
    }
}
```

### 7. Update `rv32fa/mod.rs`

Change from `pub mod alloc;` to use the new module structure.

## Validate

```bash
cargo check -p lpvm-native --lib
```

Should compile with `todo!()` placeholders.

# Phase 1: Create alloc/ Module Structure

## Scope

Create the `alloc/` directory and module files with placeholder implementations. The region tree is built in the lowerer, not in alloc/.

## Implementation

### 1. Create directory

```bash
mkdir -p lp-shader/lpvm-native/src/isa/rv32fa/alloc
```

### 2. Create `alloc/mod.rs`

```rust
//! Fast allocator shell - liveness, trace, backward walk.
//! Note: Region tree is built in lower.rs, not here.

pub mod liveness;
pub mod trace;
pub mod walk;

use alloc::vec::Vec;
use lpir::{IrFunction, VReg};
use crate::abi::FuncAbi;
use crate::vinst::VInst;
use crate::lower::Region;
use crate::isa::rv32fa::inst::PInst;
use crate::error::NativeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    UnsupportedControlFlow,
    // ... other errors
}

/// FastAlloc shell - uses region tree from lowerer, builds liveness, trace, walks backward with stubs.
pub struct FastAlloc;

impl FastAlloc {
    pub fn allocate(
        vinsts: &[VInst],
        region: &Region,
        func_abi: &FuncAbi,
        func: &IrFunction,
    ) -> Result<(Vec<PInst>, trace::AllocTrace), AllocError> {
        // TODO(Phase 2-5): Build liveness, walk, return trace
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

### 3. Create `alloc/liveness.rs`

```rust
//! Recursive liveness analysis for region tree.

use alloc::vec::Vec;
use alloc::collections::BTreeSet;
use crate::lower::Region;
use crate::vinst::{VInst, VReg};

#[derive(Debug, Clone)]
pub struct LiveSet(pub BTreeSet<VReg>);

impl LiveSet {
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }
    
    pub fn union(&self, other: &LiveSet) -> LiveSet {
        LiveSet(self.0.union(&other.0).cloned().collect())
    }
    
    pub fn remove(&mut self, vreg: VReg) {
        self.0.remove(&vreg);
    }
    
    pub fn insert(&mut self, vreg: VReg) {
        self.0.insert(vreg);
    }
}

/// Liveness result per region node.
#[derive(Debug)]
pub struct RegionLiveness {
    pub live_in: LiveSet,
    pub live_out: LiveSet,
}

/// Analyze liveness recursively on region tree.
/// M4: Stub implementation - returns empty liveness.
pub fn analyze_liveness(region: &Region, vinsts: &[VInst]) -> RegionLiveness {
    todo!()
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &RegionLiveness) -> String {
    todo!()
}
```

### 4. Create `alloc/trace.rs`

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

    /// Reverse entries (allocator walks backward, trace shown forward).
    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    /// Format as human-readable table.
    pub fn format(&self) -> String {
        todo!()
    }
}
```

### 5. Create `alloc/walk.rs`

```rust
//! Backward walk allocator shell with stubbed decisions.

use alloc::vec::Vec;
use crate::lower::Region;
use crate::isa::rv32fa::alloc::trace::{AllocTrace, TraceEntry, TraceDecision};
use crate::vinst::VInst;

/// Walk a region backward, recording stubbed decisions.
/// M4: Stub implementation.
pub fn walk_region_stub(region: &Region, vinsts: &[VInst], trace: &mut AllocTrace) {
    todo!()
}

fn stub_process_instruction(vinst_idx: usize, vinst: &VInst) -> TraceEntry {
    // STUB: Log what we would do, don't actually do it
    let decision = TraceDecision::StubAssign { vreg: 0, preg: 0 };
    let message = format!("Would process {:?} at {}", vinst.mnemonic(), vinst_idx);
    
    TraceEntry {
        vinst_idx,
        vinst: vinst.clone(),
        decision,
        message,
    }
}
```

### 6. Update `rv32fa/mod.rs`

Change from `pub mod alloc;` to use the new module structure.

### 7. Update `lower.rs` exports

Ensure `Region` is exported from `lower.rs` for use by alloc module:

```rust
// In lower.rs
pub use Region;  // Make available to alloc module
```

## Validate

```bash
cargo check -p lpvm-native --lib
```

Should compile with `todo!()` placeholders.

# Phase 1: Create alloc/ Module + Populate RegionTree

## Scope

Two tasks combined since they're tightly coupled:
1. Create the `src/alloc/` directory with placeholder modules
2. Populate the existing `RegionTree` during lowering (currently returns `RegionTree::default()`)

## Implementation

### 1. Create `src/alloc/` module structure

```bash
mkdir -p lp-shader/lpvm-native/src/alloc
```

#### `alloc/mod.rs`

```rust
//! Fast allocator shell — liveness, trace, backward walk.
//! The RegionTree is built in lower.rs; this module consumes it.

pub mod liveness;
pub mod trace;
pub mod walk;

use crate::abi::FuncAbi;
use crate::lower::LoweredFunction;
use self::trace::AllocTrace;

/// Run the allocator shell: liveness + backward walk with stubbed decisions.
/// Returns a trace of what the allocator would do (M4: stubs only).
pub fn run_shell(lowered: &LoweredFunction, _func_abi: &FuncAbi) -> AllocTrace {
    let mut trace = AllocTrace::new();

    let root = lowered.region_tree.root;
    if root != crate::region::REGION_ID_NONE {
        walk::walk_region_stub(
            &lowered.region_tree,
            root,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &mut trace,
        );
        trace.reverse();
    }

    trace
}
```

#### `alloc/liveness.rs`

```rust
//! Recursive liveness analysis for region tree.
//! Uses RegSet (fixed-size bitset, no heap).

use crate::region::{Region, RegionId, RegionTree};
use crate::regset::RegSet;
use crate::vinst::{VInst, VReg};

/// Liveness result for a region.
#[derive(Debug, Clone)]
pub struct Liveness {
    pub live_in: RegSet,
    pub live_out: RegSet,
}

/// Analyze liveness recursively on region tree.
/// M4: Only handles Linear regions.
pub fn analyze_liveness(
    _tree: &RegionTree,
    _region_id: RegionId,
    _vinsts: &[VInst],
    _pool: &[VReg],
) -> Liveness {
    // TODO(M4 Phase 3): implement
    Liveness {
        live_in: RegSet::new(),
        live_out: RegSet::new(),
    }
}
```

#### `alloc/trace.rs`

```rust
//! AllocTrace system for debugging allocator decisions.

use alloc::string::String;
use alloc::vec::Vec;

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

    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    pub fn format(&self) -> String {
        // TODO(M4 Phase 4): implement
        String::new()
    }
}
```

#### `alloc/walk.rs`

```rust
//! Backward walk allocator shell with stubbed decisions.

use crate::region::{Region, RegionId, RegionTree};
use crate::vinst::{VInst, VReg};
use super::trace::{AllocTrace, TraceEntry};

/// Walk a region backward, recording stubbed decisions to trace.
/// M4: Only handles Linear regions.
pub fn walk_region_stub(
    _tree: &RegionTree,
    _region_id: RegionId,
    _vinsts: &[VInst],
    _pool: &[VReg],
    _trace: &mut AllocTrace,
) {
    // TODO(M4 Phase 5): implement
}
```

### 2. Register `alloc` module in `lib.rs`

Add `pub mod alloc;` to `src/lib.rs`.

### 3. Populate RegionTree in lowerer

In `lower.rs`, modify `LowerCtx` to carry a `RegionTree` and build regions during `lower_range`:

- Add `region_tree: RegionTree` field to `LowerCtx`
- `lower_range` returns `Result<RegionId, LowerError>` alongside emitting VInsts
- For each LPIR op processed:
  - `IfStart` → creates `Region::IfThenElse { head, then_body, else_body }`
  - `LoopStart` → creates `Region::Loop { header, body }`
  - Default → accumulates into current linear region
- Consecutive linear regions are coalesced via helper
- `lower_ops` sets `region_tree.root` from the top-level region id

Key helper on `RegionTree`:

```rust
impl RegionTree {
    pub fn push(&mut self, region: Region) -> RegionId {
        let id = self.nodes.len() as RegionId;
        self.nodes.push(region);
        id
    }

    pub fn push_seq(&mut self, children: &[RegionId]) -> RegionId {
        let start = self.seq_children.len() as u16;
        self.seq_children.extend_from_slice(children);
        self.push(Region::Seq {
            children_start: start,
            child_count: children.len() as u16,
        })
    }
}
```

### 4. Return populated `region_tree` in `LoweredFunction`

In `lower_ops()`, change:
```rust
region_tree: RegionTree::default(),
```
to:
```rust
region_tree: ctx.region_tree,
```
with `ctx.region_tree.root` set to the root region id.

## Validate

```bash
cargo check -p lpvm-native --lib
cargo test -p lpvm-native --lib
```

Should compile and all existing tests pass. Region tree is now populated for all lowered functions.

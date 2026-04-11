# M4: Allocator Shell - Notes

## Scope of Work

Build the allocator structure: **populate the RegionTree** built during lowering, recursive liveness analysis for structured control flow, trace system, and backward walk shell. The allocator produces PInsts via the existing `rv32/alloc.rs`; the shell runs alongside it with stubbed decisions.

## Current State (Post-M3.2)

The crate was restructured in M3.2. Key changes relevant to M4:

- **Crate is `lpvm-native-fa`**, not `lpvm-native`. Module paths are `crate::rv32::` not `crate::isa::rv32fa::`.
- **`src/region.rs` exists** with arena-based `RegionTree`, `Region` enum (`Linear`, `IfThenElse`, `Loop`, `Seq`), `RegionId = u16`.
- **`src/regset.rs` exists** with `RegSet` (fixed-size `[u64; 4]` bitset over VRegs, no heap).
- **`LoweredFunction.region_tree` exists** but is currently `RegionTree::default()` (not populated).
- **`VInst::for_each_def` / `for_each_use`** exist and handle all variants including `VRegSlice`-based `Call`/`Ret` with pool parameter.
- **`VInst::mnemonic()` / `format_alloc_trace_detail()`** exist for trace formatting.
- **`compile.rs` / `emit.rs` / `link.rs`** provide the compilation pipeline. The shell doesn't replace this; it runs alongside.

The existing allocator in `rv32/alloc.rs` is a simple forward-walk:
- Single-pass forward allocation with last-use register freeing
- Basic parameter precoloring
- Works for straight-line code
- ~690 lines including tests

## Design: Arena-Based RegionTree

The `Region` enum uses arena indices (not `Box<Region>` as the original plan suggested):

```rust
pub type RegionId = u16;

pub enum Region {
    Linear { start: u16, end: u16 },
    IfThenElse { head: RegionId, then_body: RegionId, else_body: RegionId },
    Loop { header: RegionId, body: RegionId },
    Seq { children_start: u16, child_count: u16 },
}

pub struct RegionTree {
    pub nodes: Vec<Region>,
    pub seq_children: Vec<RegionId>,
    pub root: RegionId,
}
```

Benefits over Box<Region>:
- No individual heap allocations per node
- Cache-friendly (contiguous Vec)
- Better for `no_std` / embedded
- Seq children stored separately, avoiding variable-size enum variants

## Questions

### Q1: Should we keep the current simple allocator?

**Answer:** Yes, for M4. The simple allocator in `rv32/alloc.rs` continues producing PInsts. The new shell runs alongside it to demonstrate the infrastructure. M5 replaces the simple allocator with the new backward walk.

### Q2: Liveness data structure?

**Answer:** Use `RegSet` from `regset.rs`. This is a fixed-size `[u64; VREG_WORDS]` bitset (32 bytes for 256 vregs). Zero heap allocation. Already has `insert`, `remove`, `contains`, `union`, `iter`.

Do NOT use `BTreeSet<VReg>` as the original plan suggested — that allocates on the heap and is slower for this fixed-size domain.

### Q3: How does the lowerer build the RegionTree?

**Answer:** Extend `lower_range` to return a `RegionId`:

1. Walk LPIR ops in range
2. For each `IfStart`: recursively lower then/else bodies, create `IfThenElse` node
3. For each `LoopStart`: recursively lower body, create `Loop` node
4. For plain ops: accumulate VInst indices, create `Linear` node
5. Coalesce consecutive `Linear` nodes
6. If multiple children: wrap in `Seq` node

The tree is built during the existing lowering pass at zero extra cost.

### Q4: VInst shape differences from original plan?

The original plan assumed `VInst::Call { name: String, ... }` and `VInst::Ret { vals: Vec<VReg>, ... }`. Actual shapes:

```rust
VInst::Call {
    target: SymbolId,       // index into ModuleSymbols, not a String
    args: VRegSlice,        // pool-based, not Vec
    rets: VRegSlice,        // pool-based, not Vec
    callee_uses_sret: bool,
    src_op: u16,
}

VInst::Ret {
    vals: VRegSlice,        // pool-based, not Vec
    src_op: u16,
}
```

The walk/trace code must use `pool` parameter for `for_each_use`/`for_each_def` and `ModuleSymbols` for callee name resolution.

### Q5: How does recursive liveness work with RegSet?

```rust
fn analyze_liveness(tree: &RegionTree, id: RegionId, vinsts: &[VInst], pool: &[VReg]) -> Liveness {
    match &tree.nodes[id as usize] {
        Region::Linear { start, end } => {
            let mut live = RegSet::new();
            for i in (*start..*end).rev() {
                vinsts[i as usize].for_each_def(pool, |d| live.remove(d));
                vinsts[i as usize].for_each_use(pool, |u| live.insert(u));
            }
            Liveness { live_in: live, live_out: RegSet::new() }
        }
        Region::IfThenElse { head, then_body, else_body } => {
            let then_l = analyze_liveness(tree, *then_body, vinsts, pool);
            let else_l = analyze_liveness(tree, *else_body, vinsts, pool);
            let merge = then_l.live_in.union(&else_l.live_in);
            let head_l = analyze_liveness(tree, *head, vinsts, pool);
            Liveness { live_in: merge.union(&head_l.live_in), live_out: RegSet::new() }
        }
        // ...
    }
}
```

Key: uses `VInst::for_each_def` / `for_each_use` which already handle all variants correctly.

## M4 → M5 Boundary

**M4 produces:**
- Populated `RegionTree` with correct structure
- `AllocTrace` with stubbed decisions
- Liveness analysis for Linear/Seq regions (IfThenElse/Loop conservative)
- CLI `--show-region` and `--show-liveness`

**M5 replaces:**
- Stubbed walk with real register assignment (LRU, spill, reload)
- Conservative IfThenElse/Loop liveness with correct handling
- `rv32/alloc.rs` simple allocator with the new backward walk allocator

## Notes

- The existing `rv32/alloc.rs` is ~690 lines with tests
- The new structure is modular: `alloc/mod.rs`, `alloc/liveness.rs`, `alloc/trace.rs`, `alloc/walk.rs`
- Textual debug output is critical — every IR stage must be printable
- The trace is reversible: allocator walks backward, trace shown forward
- Region tree coalescing: merge consecutive Linear regions to keep tree compact
- All liveness uses `RegSet` (no heap), not `BTreeSet`

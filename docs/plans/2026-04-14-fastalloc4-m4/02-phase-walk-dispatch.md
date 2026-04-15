# Phase 2: Allocator — walk_region Dispatch + Seq + boundary_spill

## Scope

Add the recursive `walk_region` dispatcher, the `boundary_spill` helper, and
`Seq` region handling. This is the foundation for IfThenElse and Loop in later
phases. Update `allocate()` to call `walk_region` instead of `walk_linear`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### `fa_alloc/walk.rs` — shared state struct

Currently `walk_linear` creates local `SpillAlloc`, `AllocTrace`, `edits`, and
`allocs` then passes them to `process_generic` / `process_call`. For the
recursive walk, these need to be shared across region visits. Extract into a
struct:

```rust
struct WalkState<'a> {
    vinsts: &'a [VInst],
    vreg_pool: &'a [VReg],
    func_abi: &'a FuncAbi,
    tree: &'a RegionTree,
    pool: RegPool,
    spill: SpillAlloc,
    allocs: Vec<Alloc>,
    inst_alloc_offsets: Vec<usize>,
    edits: Vec<(EditPoint, Edit)>,
    trace: AllocTrace,
}
```

### `walk_region` dispatcher

```rust
impl<'a> WalkState<'a> {
    fn walk_region(&mut self, region_id: RegionId) -> Result<(), AllocError> {
        if region_id == REGION_ID_NONE {
            return Ok(());
        }
        let region = self.tree.nodes[region_id as usize].clone();
        match region {
            Region::Linear { start, end } => self.walk_linear_range(start, end),
            Region::Seq { children_start, child_count } => self.walk_seq(children_start, child_count),
            Region::IfThenElse { .. } => Err(AllocError::UnsupportedControlFlow), // Phase 3
            Region::Loop { .. } => Err(AllocError::UnsupportedControlFlow),       // Phase 4
        }
    }
}
```

### `walk_linear_range` — extract from walk_linear

Move the main backward-walk loop from `walk_linear` into
`WalkState::walk_linear_range(start, end)`. This walks `vinsts[start..end]`
backward using the shared pool/spill/edits state. The existing `process_generic`
and `process_call` functions remain mostly unchanged but operate on `&mut self`
fields.

### `boundary_spill` helper

```rust
impl<'a> WalkState<'a> {
    /// Spill all pool-resident vregs that are live in `live_in` to their
    /// spill slots. Free all pool entries. After this, the pool is empty.
    fn boundary_spill(&mut self, live_in: &RegSet, anchor_idx: u16) {
        for preg in self.pool.occupied_pregs() {
            if let Some(vreg) = self.pool.vreg_in(preg) {
                if live_in.contains(vreg) {
                    let slot = self.spill.get_or_assign(vreg);
                    self.edits.push((
                        EditPoint::After(anchor_idx),
                        Edit::Move {
                            from: Alloc::Reg(preg),
                            to: Alloc::Stack(slot),
                        },
                    ));
                }
            }
        }
        self.pool.clear();
    }
}
```

Requires adding to `RegPool`:
- `occupied_pregs() -> impl Iterator<Item = PReg>` — iterate pregs with a vreg
- `vreg_in(preg) -> Option<VReg>` — get vreg occupying preg
- `clear()` — free all pregs

### `walk_seq`

```rust
fn walk_seq(&mut self, children_start: u16, child_count: u16) -> Result<(), AllocError> {
    let start = children_start as usize;
    let end = start + child_count as usize;
    let children: Vec<RegionId> = self.tree.seq_children[start..end].to_vec();

    // Walk children in reverse (backward walk)
    for (i, &child_id) in children.iter().enumerate().rev() {
        self.walk_region(child_id)?;
        // Boundary spill between children (not after the last one in backward order)
        if i > 0 {
            // Compute live_in for the previous child (backward = next in forward order)
            let prev_child = children[i - 1];
            let live = analyze_liveness(self.tree, prev_child, self.vinsts, self.vreg_pool);
            // anchor_idx: first instruction of current child
            let anchor = self.region_first_inst(child_id);
            self.boundary_spill(&live.live_in, anchor);
        }
    }
    Ok(())
}
```

### `allocate()` update in `fa_alloc/mod.rs`

Replace the current Linear-only match with:

```rust
pub fn allocate(lowered: &LoweredFunction, func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    let mut state = WalkState::new(lowered, func_abi);
    state.walk_region(lowered.region_tree.root)?;
    state.finalize(func_abi)
}
```

Where `finalize` does the entry-move logic (precolors, stack params) and builds
the `AllocResult`.

### Tests

Unit tests using the builder:

1. **seq_two_linears**: Two linear blocks in a Seq. First block: `IConst32`,
   second block: use the value. Verify boundary spill/reload edits appear.

2. **seq_three_with_spill**: Three blocks, values flow across boundaries.
   Check that dead values at boundaries are freed without stores.

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native
```

All existing linear-only tests must still pass (the `walk_region → Linear`
path should be equivalent). The Seq tests should pass with boundary spills.
IfThenElse/Loop still return `UnsupportedControlFlow`.

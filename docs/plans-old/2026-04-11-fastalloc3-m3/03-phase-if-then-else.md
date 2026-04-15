# Phase 3: IfThenElse Walk

## Scope

Implement `Region::IfThenElse` handling in `walk_region`. Register state is
saved before walking one branch, restored before walking the other, and
reconciled at the head.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Add `RegPool::snapshot` / `RegPool::restore`

```rust
impl RegPool {
    /// Snapshot current register assignments.
    pub fn snapshot(&self) -> [Option<VReg>; 32] {
        self.preg_vreg
    }

    /// Restore register assignments from a snapshot.
    pub fn restore(&mut self, snap: [Option<VReg>; 32]) {
        self.preg_vreg = snap;
        // Rebuild LRU from snapshot: free regs first, then occupied
        self.lru.clear();
        for &p in ALLOC_POOL.iter() {
            if self.preg_vreg[p as usize].is_none() {
                self.lru.push(p);
            }
        }
        for &p in ALLOC_POOL.iter() {
            if self.preg_vreg[p as usize].is_some() {
                self.lru.push(p);
            }
        }
    }
}
```

### 2. Add reconciliation helper

When two branches produce different register states at the merge point, emit
moves to align the non-canonical branch to match the canonical one.

Strategy: for any vreg that is in a register in one branch but a different
register (or absent) in the other, spill it in the non-canonical branch.
The lazy reload mechanism handles reloading it when needed.

```rust
/// Emit fixup instructions to reconcile `current` pool state with `target`.
/// Called after walking the non-canonical branch. Emits spills for vregs
/// that are in different registers than `target` expects.
fn reconcile_to_target(
    state: &mut WalkState,
    target_snap: &[Option<VReg>; 32],
) {
    // For each occupied reg in the current pool, if the target expects a
    // different vreg there (or none), spill the current occupant.
    for &preg in ALLOC_POOL.iter() {
        let current = state.pool.preg_vreg[preg as usize];
        let target = target_snap[preg as usize];
        if current != target {
            if let Some(vreg) = current {
                // Current branch has vreg here but target doesn't — spill it
                let slot = state.spill.get_or_assign(vreg);
                let offset = -((slot as i32 + 1) * 4);
                state.pinsts.push(PInst::Sw {
                    src: preg,
                    base: FP_REG,
                    offset,
                });
            }
        }
    }
}
```

### 3. Implement IfThenElse in `walk_region`

```rust
Region::IfThenElse { head, then_body, else_body } => {
    // Save the merge-point pool state (state arriving from after the if/else)
    let merge_snap = state.pool.snapshot();

    // Walk else_body backward
    walk_region(state, tree, *else_body, vinsts, vreg_pool, func_abi)?;
    let else_snap = state.pool.snapshot();

    // Restore merge-point state, walk then_body backward
    state.pool.restore(merge_snap);
    walk_region(state, tree, *then_body, vinsts, vreg_pool, func_abi)?;
    // state now reflects then-branch state (canonical)

    let then_snap = state.pool.snapshot();

    // Reconcile else branch to match then-branch state
    // Insert fixup instructions in the else branch's PInst segment
    // For simplicity: emit spills into the current pinst stream for vregs
    // that differ. Since the else body was walked first and its pinsts are
    // already in the stream, we insert reconciliation at the boundary.
    //
    // Actually: we need to insert the reconciliation moves into the else
    // branch's instruction range, not the current position. A simpler approach:
    // after walking else, reconcile immediately (before restoring for then).
    // Let's restructure:

    // --- Revised approach ---
    // 1. Walk else_body
    // 2. Record else pinst range
    // 3. Restore merge state
    // 4. Walk then_body
    // 5. Now we know the canonical (then) state
    // 6. Emit reconciliation for else at the recorded position
    //
    // This requires inserting into the middle of pinsts, which is expensive.
    // Simpler: walk then first, then else, reconcile else immediately.

    // Walk head
    walk_region(state, tree, *head, vinsts, vreg_pool, func_abi)?;

    Ok(())
}
```

**Revised approach** (walk then first, then else):

Since we're building the PInst stream in reverse and will reverse it at the end,
the order of walking branches doesn't matter for correctness — what matters is
that reconciliation moves are placed in the right position.

Simpler strategy:
1. Save merge-point snapshot
2. Walk then_body → produces then_snap
3. Restore merge-point snapshot
4. Walk else_body
5. Emit reconciliation moves (spills for disagreeing vregs) at current position
   (which maps to the start of else_body in execution order)
6. Restore then_snap as the canonical state for the head
7. Walk head

This way reconciliation moves are naturally appended after else_body in the
backward stream, which means they appear at the beginning of the else block
in execution order (right after the else label).

### 4. Update liveness for IfThenElse

In `liveness.rs`, update the IfThenElse arm to compute proper live_in:

```rust
Region::IfThenElse { head, then_body, else_body } => {
    let head_liveness = analyze_liveness(tree, *head, vinsts, pool);
    let then_liveness = analyze_liveness(tree, *then_body, vinsts, pool);
    let else_liveness = analyze_liveness(tree, *else_body, vinsts, pool);

    // live_in = head_live_in ∪ then_live_in ∪ else_live_in
    let combined = head_liveness.live_in
        .union(&then_liveness.live_in)
        .union(&else_liveness.live_in);

    Liveness {
        live_in: combined,
        live_out: RegSet::new(),
    }
}
```

## Tests

- Unit test: IfThenElse with both branches using the same vreg — no reconciliation needed.
- Unit test: IfThenElse where then assigns v0→t4, else assigns v0→t5 — reconciliation
  spills v0 in else branch.
- Unit test: nested IfThenElse — inner if/else within a then_body.
- Existing straight-line tests still pass.

## Validate

```bash
cargo test -p lpvm-native
cargo check -p lpvm-native
```

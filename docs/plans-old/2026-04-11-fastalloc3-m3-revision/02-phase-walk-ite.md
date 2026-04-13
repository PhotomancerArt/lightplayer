# Phase 2: RegPool Boundary Helpers + walk_ite

## Scope of phase

Implement the spill-at-boundary helpers on `WalkState` and the IfThenElse
backward walker. After this phase, if/else functions work.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Add boundary helpers to RegPool

In `fa_alloc/walk.rs`, add a method to get a snapshot of occupied regs:

```rust
impl RegPool {
    /// Get current occupied (preg, vreg) pairs for saving state.
    pub fn snapshot_occupied(&self) -> Vec<(PReg, VReg)> {
        ALLOC_POOL.iter()
            .filter_map(|&p| {
                self.preg_vreg[p as usize].map(|v| (p, v))
            })
            .collect()
    }

    /// Clear the pool (free all registers).
    pub fn clear(&mut self) {
        for p in ALLOC_POOL.iter() {
            self.preg_vreg[*p as usize] = None;
        }
        // Reset LRU to initial state (free regs first)
        self.lru.clear();
        self.lru.extend(ALLOC_POOL.iter().copied());
    }

    /// Seed pool with vreg assignments from saved state.
    /// Any existing occupants are evicted (spilled) by the caller.
    pub fn seed(&mut self, assignments: &[(PReg, VReg)]) {
        self.clear();
        for &(preg, vreg) in assignments {
            self.preg_vreg[preg as usize] = Some(vreg);
            self.touch(preg);
        }
    }
}
```

### 2. Add boundary helpers to WalkState

```rust
impl<'a> WalkState<'a> {
    /// Flush all occupied registers to spill slots.
    /// Returns saved state (vec of (preg, vreg) assignments) for later seeding.
    /// Emits Lw instructions (reloads in forward order).
    fn flush_to_slots(&mut self) -> Vec<(PReg, VReg)> {
        let occupied = self.pool.snapshot_occupied();
        let saved = occupied.clone();
        
        for (preg, vreg) in occupied {
            let slot = self.spill.get_or_assign(vreg);
            let offset = -((slot as i32 + 1) * 4);
            self.pinsts.push(PInst::Lw { dst: preg, base: FP_REG, offset });
        }
        
        self.pool.clear();
        saved
    }

    /// Emit Sw (spill) for all occupied registers.
    /// Pool is NOT cleared — backward walk needs vregs registered.
    fn emit_exit_spills(&mut self) {
        for (preg, vreg) in self.pool.iter_occupied().collect::<Vec<_>>() {
            let slot = self.spill.get_or_assign(vreg);
            let offset = -((slot as i32 + 1) * 4);
            self.pinsts.push(PInst::Sw { src: preg, base: FP_REG, offset });
        }
    }

    /// Seed pool from saved state (vreg → preg assignments).
    fn seed_pool(&mut self, saved: &[(PReg, VReg)]) {
        self.pool.seed(saved);
    }
}
```

### 3. Implement walk_ite

Add to `walk_region` match arm for `IfThenElse`:

```rust
Region::IfThenElse { head, then_body, else_body, else_label, merge_label } => {
    self.walk_ite(
        state, tree, *head, *then_body, *else_body,
        *else_label, *merge_label,
        vinsts, vreg_pool, func_abi
    )
}
```

The `walk_ite` function implements the backward push ordering:

```rust
fn walk_ite(
    &self,
    state: &mut WalkState<'_>,
    tree: &RegionTree,
    head: RegionId,
    then_body: RegionId,
    else_body: RegionId,
    else_label: LabelId,
    merge_label: LabelId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    // 1. Flush merge-live vregs to spill slots (emits Lw)
    let merge_live = state.flush_to_slots();

    // 2. Walk else body (if non-empty)
    if else_body != REGION_ID_NONE {
        // Seed pool with merge_live
        state.seed_pool(&merge_live);
        // Emit Sw for exit (appears at else exit in forward)
        state.emit_exit_spills();
        // Walk else computation
        self.walk_region(state, tree, else_body, vinsts, vreg_pool, func_abi)?;
        // Flush else entry (emits Lw)
        let _else_entry_live = state.flush_to_slots();
    }

    // 3. Emit else label and J(merge)
    state.pinsts.push(PInst::J { target: merge_label });
    state.pinsts.push(PInst::Label { id: else_label });

    // 4. Walk then body
    state.seed_pool(&merge_live);
    state.emit_exit_spills();
    self.walk_region(state, tree, then_body, vinsts, vreg_pool, func_abi)?;
    let _then_entry_live = state.flush_to_slots();

    // 5. Emit head exit spills, then walk head
    state.seed_pool(&merge_live);
    state.emit_exit_spills();
    self.walk_region(state, tree, head, vinsts, vreg_pool, func_abi)?;

    // 6. Done — merge_live is the canonical state at head entry
    // The pool now reflects state after head walk
    Ok(())
}
```

Wait — need to rethink the ordering. The above pushes in wrong order for
the backward walk. Let me trace through carefully.

Forward execution order:
```
[head] → (branch) → [then] → [Sw] → [J merge] → [else_label] → [else] → [Sw] → [merge]
```

Backward walk processes from merge backward. So we need:
```rust
// Process merge point (already have pool state from "rest")
let merge_live = state.flush_to_slots();  // Lw reloads

// Process else body (executes second in forward, processed first in backward)
// In forward: else_label, Lw entry, else comp, Sw exit
// In backward: Sw exit, else comp, Lw entry, else_label
state.seed_pool(&merge_live);
state.emit_exit_spills();  // Sw at else exit
walk_region(else_body);    // else computation
state.flush_to_slots();    // Lw at else entry (but we need else_label before this!)

// Hmm, the else_label comes BEFORE else entry in forward.
// So in backward push order: else_label comes AFTER else entry Lw.
// Actually labels have no effect on register state, so we can emit
// them anywhere as long as they're at the right position in the final stream.
```

The correct backward push sequence for non-empty else:

```rust
// We're at merge point, pool has live vregs from "rest"
let merge_live = state.flush_to_slots();  // emits Lw (reload at merge)

// Push order (last = first in forward):
state.pinsts.push(PInst::Label { id: merge_label });  // Forward: at merge

// Else branch (processed first in backward = appears last in forward)
state.seed_pool(&merge_live);
state.emit_exit_spills();  // Forward: Sw at else exit
walk_region(state, tree, else_body, ...)?;  // Forward: else computation
let else_entry = state.flush_to_slots();  // Forward: Lw at else entry
// (we ignore else_entry return value, pool is cleared)

// Then push else_label (appears before else entry in forward)
state.pinsts.push(PInst::Label { id: else_label });

// Then branch (processed second in backward = appears before else in forward)
state.seed_pool(&merge_live);
state.emit_exit_spills();  // Forward: Sw at then exit
walk_region(state, tree, then_body, ...)?;  // Forward: then computation
let _then_entry = state.flush_to_slots();  // Forward: Lw at then entry

// J to merge (at end of then in forward)
state.pinsts.push(PInst::J { target: merge_label });

// Head (processed last in backward = first in forward)
state.seed_pool(&merge_live);
state.emit_exit_spills();  // Forward: Sw at head exit
walk_region(state, tree, head, ...)?;  // Forward: head (includes BrIf)
// Pool now has state at function start (or before head if nested)

Ok(())
```

Wait, that's wrong. The `flush_to_slots` emits Lw (reload), but we need Sw
(spill) at branch exits. Let me re-trace.

Actually, the boundary convention is:
- At the START of a region in forward order, values are in their spill slots.
- The region entry code (Lw) loads them into registers for use.
- At the END of the region, we spill back to slots (Sw) before the next region.

So the forward order is:
```
[head]
  [Sw head exit]  ← boundary: spill live vregs to slots
[then entry Lw] [then comp] [Sw then exit]  ← then body
[J merge]
[else_label]
[else entry Lw] [else comp] [Sw else exit]  ← else body
[merge_label]
[merge entry Lw] [rest...]  ← merge point
```

In backward walk order (processing from end to start):
1. Process "rest" — pool has vregs used after merge
2. At merge boundary: emit Lw (reload into regs for use in rest)
   Wait no — in backward, we encounter uses first, so values should be in regs.
   At the merge point boundary in backward, we need to transition from
   "vregs in regs" (for uses in rest) to "vregs in slots" (boundary invariant).
   So we emit Sw! Because Sw in backward stream becomes Lw in forward.

Let me think more carefully about backward vs forward:

Backward walk: process instructions from end to start.
We push PInsts to a Vec. At the end, we reverse the Vec to get forward order.

So if in backward walk I push [A, B, C], after reverse I get [C, B, A].

For a value that's in a spill slot at the boundary:
- In forward: Lw at entry (load into reg), use in body, Sw at exit (spill)
- In backward: Sw at exit (pushed first), use in body (regs), Lw at entry (pushed last)

So for boundary transitions:
- `flush_to_slots()` in backward walk: emit Lw (reload) — because after
  reversal, this becomes the entry reload. The vregs go from "in regs" (for
  uses in prior backward code) to "in slots" (boundary invariant).

Hmm wait, that's confusing. Let me just trace a concrete example.

Suppose v0 is live across an if/else. In forward:
```
head:   (v0 in t0)
Sw v0   (spill to slot 0)
then:   Lw v0, use v0, Sw v0
else:   Lw v0, use v0, Sw v0
merge:  Lw v0, use v0
```

In backward, processing merge→head:
1. At merge: v0 used — allocated to t0
2. Walk backward toward merge entry: at boundary, need v0 in slot
   Push Lw (will become Sw after reversal? No...)

Actually I think I had it backwards. Let me be very explicit.

The `pinsts` Vec is built in backward order and reversed at the end.
So:
- Forward instruction at position N is pushed at position (len-1-N) in backward

If I want forward order: [A, B, C, D]
I push in backward: [D, C, B, A], then reverse to get [A, B, C, D].

So for if/else forward layout:
```
[head] [Sw] [then_Lw] [then] [Sw] [J] [else_label] [else_Lw] [else] [Sw] [merge_Lw]
```

Backward push order (first pushed = last in forward):
```rust
// Process merge: v0 in regs for uses in rest
// At merge boundary: emit merge_Lw (will be last in forward)
state.pinsts.push(PInst::Lw { ... });  // merge reload

// Process else: v0 in slots at entry
state.seed_pool(&[(t0, v0)]);  // pretend v0 was in t0
state.emit_exit_spills();  // push Sw — becomes early in forward (else exit)
walk_else();  // v0 used, in t0
state.flush_to_slots();  // push Lw — becomes later in forward (else entry)

// Push else_label
state.pinsts.push(PInst::Label { id: else_label });

// Process then
state.seed_pool(&[(t0, v0)]);
state.emit_exit_spills();  // Sw — then exit
walk_then();  // v0 used
state.flush_to_slots();  // Lw — then entry

// Push J
state.pinsts.push(PInst::J { target: merge });

// Process head
state.seed_pool(&[(t0, v0)]);
state.emit_exit_spills();  // Sw — head exit
walk_head();  // includes BrIf
// Done
```

After reverse, forward order:
```
[head] [Sw head exit] [then Lw] [then] [Sw then exit] [J] [else_label] [else Lw] [else] [Sw else exit] [merge Lw]
```

That looks right! So:
- `flush_to_slots()` emits Lw (reload in forward, so vregs go from "in regs" to "in slots")
- `emit_exit_spills()` emits Sw (spill in forward, so vregs stay in regs but Sw is emitted)

Actually wait, I think the naming is confusing me. Let me rename:

- `emit_boundary_reload()` — emit Lw (vreg in slot → vreg in reg, for entry)
- `emit_boundary_spill()` — emit Sw (vreg in reg → vreg in slot, for exit)

But in the backward walk:
- At region EXIT in forward (where we need Sw), we process it at region START in backward
- At region ENTRY in forward (where we need Lw), we process it at region END in backward

So the backward walk function structure:
```rust
fn walk_ite(...) {
    // Process from merge backward

    // 1. At merge point (end of "rest" in backward = start of merge in forward)
    //    We have vregs in regs for uses in "rest"
    //    At merge boundary, they need to go to slots
    //    In forward: Lw at merge entry (load from slots)
    //    In backward: we're at merge "start" — but backward processes end→start
    //    So we've already processed the "rest" code, and now at the boundary
    //    we need to transition TO slots (for the boundary invariant)
    //    This means emitting Lw (which after reversal becomes the reload)

    // Hmm I keep getting confused. Let me use concrete push calls.

    // Current state: pool has vregs live in "rest" (after merge)
    // These vregs need to be in slots at the merge boundary
    // In forward: after merge, we have vregs in slots, then Lw loads them
    // In backward: before processing merge, we need vregs in slots
    //              so we emit Lw here (pushed now, appears at end after reversal)

    // Actually simpler: just implement and trace through.
}
```

I think the confusion comes from "boundary" having different meanings. Let me
just implement `boundary_exit()` and `boundary_entry()` and test.

For now, implement the helpers and a simple test, then iterate.

### 4. Update liveness for IfThenElse

In `liveness.rs`, the IfThenElse arm should compute:
- `live_in = head_live_in ∪ then_live_in ∪ else_live_in`
- The merge point liveness is implicit (handled by boundary convention)

Actually with spill-at-boundary, liveness analysis becomes simpler — we just
need to know which vregs are live at each region entry. The boundary
convention handles the rest.

```rust
Region::IfThenElse { head, then_body, else_body, .. } => {
    let head_liveness = analyze_liveness(tree, *head, vinsts, pool);
    let then_liveness = analyze_liveness(tree, *then_body, vinsts, pool);
    let else_liveness = if *else_body != REGION_ID_NONE {
        analyze_liveness(tree, *else_body, vinsts, pool)
    } else {
        Liveness { live_in: RegSet::new(), live_out: RegSet::new() }
    };

    // live_in = head_live_in ∪ then_live_in ∪ else_live_in
    let mut combined = head_liveness.live_in;
    combined.union(&then_liveness.live_in);
    combined.union(&else_liveness.live_in);

    Liveness {
        live_in: combined,
        live_out: RegSet::new(), // not used with boundary convention
    }
}
```

## Tests

- Unit test: `RegPool::snapshot_occupied`, `clear`, `seed`
- Unit test: `WalkState::flush_to_slots` emits correct Lw sequence
- Unit test: `WalkState::emit_exit_spills` emits correct Sw sequence
- Unit test: simple IfThenElse with one live vreg across branches
- Unit test: IfThenElse with different vregs in each branch
- Integration: simple if/else filetest passes

## Validate

```bash
cargo test -p lpvm-native-fa
cargo check -p lpvm-native-fa
```

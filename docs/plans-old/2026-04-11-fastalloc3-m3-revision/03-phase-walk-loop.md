# Phase 3: walk_loop

## Scope of phase

Implement Loop backward walker using the same spill-at-boundary pattern as
IfThenElse.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Add walk_loop match arm

In `walk_region`, add the Loop arm:

```rust
Region::Loop { header, body, header_label, exit_label } => {
    self.walk_loop(
        state, tree, *header, *body, *header_label, *exit_label,
        vinsts, vreg_pool, func_abi
    )
}
```

### 2. Implement walk_loop

Forward execution order for a loop:
```
[header_label]
[header] (loop condition)
[J exit] or fall through to body
[body] (loop body)
[J header] (back-edge)
[exit_label]
[post-loop code]
```

With spill-at-boundary:
- At header entry: live vregs in slots → Lw reload → use in header
- At header exit (before body): Sw spill → body entry Lw reload
- At body exit (before back-edge): Sw spill
- Back-edge J to header_label
- At exit: Lw reload for post-loop code

Backward push order:
```rust
fn walk_loop(
    &self,
    state: &mut WalkState<'_>,
    tree: &RegionTree,
    header: RegionId,
    body: RegionId,
    header_label: LabelId,
    exit_label: LabelId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    // 1. Flush post-loop live vregs to slots (Lw for post-loop reload)
    let post_loop_live = state.flush_to_slots();

    // 2. Emit exit label
    state.pinsts.push(PInst::Label { id: exit_label });

    // 3. Emit back-edge J
    state.pinsts.push(PInst::J { target: header_label });

    // 4. Walk body
    state.seed_pool(&post_loop_live);
    state.emit_exit_spills();  // Sw at body exit
    self.walk_region(state, tree, body, vinsts, vreg_pool, func_abi)?;
    let _body_entry = state.flush_to_slots();  // Lw at body entry

    // 5. Walk header
    state.seed_pool(&post_loop_live);
    state.emit_exit_spills();  // Sw at header exit (before body)
    self.walk_region(state, tree, header, vinsts, vreg_pool, func_abi)?;
    // After header walk, pool has state at header entry

    // 6. Emit header label
    state.pinsts.push(PInst::Label { id: header_label });

    Ok(())
}
```

Wait, this has the same ordering issue as IfThenElse. Let me trace through
carefully.

Forward order:
```
[header_label] [header Lw] [header] [Sw] [body Lw] [body] [Sw] [J header] [exit_label] [post Lw] [post]
```

Backward push (what we actually emit, then reverse):
```rust
// Process post-loop: vregs in regs for post-loop uses
// At post-loop boundary: need vregs in slots
state.flush_to_slots();  // emits Lw (post-loop reload in forward)

// Push exit_label (appears before post-loop in forward)
state.pinsts.push(PInst::Label { id: exit_label });

// Process body (executes before post-loop in forward)
state.seed_pool(&post_loop_live);
state.emit_exit_spills();  // Sw at body exit
walk_body();  // body computation
state.flush_to_slots();  // Lw at body entry

// Push J to header (at end of body in forward)
state.pinsts.push(PInst::J { target: header_label });

// Process header (executes before body in forward)
state.seed_pool(&post_loop_live);
state.emit_exit_spills();  // Sw at header exit
walk_header();  // header computation (includes condition, BrIf/J exit)
// Don't flush — header entry is the loop entry point

// Push header_label (at start of header in forward)
state.pinsts.push(PInst::Label { id: header_label });
```

After reverse, forward order:
```
[header_label] [header] [Sw] [body Lw] [body] [Sw] [J header] [exit_label] [post Lw]
```

Hmm, missing the header Lw. The issue is that the header entry boundary
should have a reload too.

Actually, in the backward walk model:
- When we START walking a region (in backward), we should have vregs in regs
- At the END of walking a region (still in backward), we emit boundary spill

So the pattern is:
```rust
// Before walking region: vregs are in regs (from previous backward code)
// Walk the region
walk_region(...)?;
// At region entry (in forward) = end of region (in backward):
// emit boundary spill (Sw in forward = what we push now)
state.emit_exit_spills();
// Then transition: vregs to slots
state.flush_to_slots();  // emits Lw in backward = reload in forward at next region entry
```

But this means the reload happens at the NEXT region's entry, not the
current region's entry.

Let me think about the header specifically. In forward:
1. header_label
2. Lw reload (header entry boundary)
3. header computation
4. Sw spill (header exit boundary)

In backward:
1. Process header computation (backward)
2. At header exit: emit Sw
3. At header entry: emit Lw
4. Emit header_label

So:
```rust
walk_header();  // processes header computation backward
state.emit_exit_spills();  // Sw at header exit
state.flush_to_slots();    // Lw at header entry
state.pinsts.push(PInst::Label { id: header_label });
```

But wait, `flush_to_slots` clears the pool. After header, we need to continue
to body. The pool should have vregs in regs for body uses.

Actually I think the boundary transition should be:
- After walking a region, the pool has vregs in regs (from uses in that region)
- We emit Sw to spill them (they go to slots)
- We "save" the assignments
- Before walking the next region, we "restore" (seed) from saved assignments
- We emit Lw to reload them (they go back to regs)
- Then walk the next region

So the pattern between regions is: [Sw] [Lw] with the same vregs.

For loop:
```rust
// After post-loop: pool has post-loop live vregs
// Spill them (boundary before header entry)
state.emit_exit_spills();  // Sw
let saved = state.pool.snapshot_occupied();
state.pool.clear();  // or keep for seeding

// Before header: seed and reload
state.seed_pool(&saved);
// Actually we need Lw here — but we just spilled, so Lw is redundant
// The Lw is for when we ENTER the loop from outside
// But we're walking backward from post-loop

// I think the key insight is: the boundary convention is about the FORWARD
// flow. In backward, we just need to maintain it.

// Let me look at how regalloc2 does it more carefully...
// Actually, I'll just implement and test with a concrete example.
```

For this phase, implement the basic structure and add tests. The boundary
logic can be refined.

### 3. Update liveness for Loop

```rust
Region::Loop { header, body, .. } => {
    let header_liveness = analyze_liveness(tree, *header, vinsts, pool);
    let body_liveness = analyze_liveness(tree, *body, vinsts, pool);

    // For now, simple union (may over-approximate but safe)
    let mut combined = header_liveness.live_in;
    combined.union(&body_liveness.live_in);

    Liveness {
        live_in: combined,
        live_out: RegSet::new(),
    }
}
```

A precise liveness would iterate to convergence for the back-edge, but with
spill-at-boundary, over-approximation is safe (just more spills).

## Tests

- Unit test: simple loop with one vreg live across iterations
- Unit test: nested loops
- Integration: simple loop filetest passes

## Validate

```bash
cargo test -p lpvm-native
cargo check -p lpvm-native
```

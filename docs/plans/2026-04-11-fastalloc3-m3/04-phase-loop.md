# Phase 4: Loop Walk

## Scope

Implement `Region::Loop` handling in `walk_region`. Single-pass backward walk
with fixup moves at the back-edge if register state at the body-end doesn't
match the header.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Implement Loop in `walk_region`

```rust
Region::Loop { header, body } => {
    // In the backward walk, we visit body then header.
    // The body may contain branches, calls, nested loops.
    //
    // Strategy:
    // 1. Save pre-loop state (from instructions after the loop)
    // 2. Walk body backward
    // 3. Walk header backward
    // 4. Compare back-edge state (pre-loop snapshot) with header state
    // 5. If they differ, emit fixup moves at the back-edge position

    let pre_loop_snap = state.pool.snapshot();

    // Walk body
    walk_region(state, tree, *body, vinsts, vreg_pool, func_abi)?;

    // Walk header
    walk_region(state, tree, *header, vinsts, vreg_pool, func_abi)?;

    let header_snap = state.pool.snapshot();

    // The back-edge needs to carry values from the pre-loop state into
    // the header. Emit fixup moves for any disagreements.
    // These fixups go at the back-edge position (just before the Br to header
    // in execution order). In our backward stream, they're emitted now and
    // will end up in the right place after reversal.
    //
    // For each vreg that is in a different register in pre_loop_snap vs
    // header_snap, emit a Mv or spill to reconcile.
    emit_loop_backedge_fixups(state, &pre_loop_snap, &header_snap);

    Ok(())
}
```

### 2. Implement `emit_loop_backedge_fixups`

```rust
fn emit_loop_backedge_fixups(
    state: &mut WalkState,
    backedge_snap: &[Option<VReg>; 32],
    header_snap: &[Option<VReg>; 32],
) {
    // For each vreg that the header expects in a register but the back-edge
    // has in a different register (or spilled), emit a fixup.
    for &preg in ALLOC_POOL.iter() {
        let at_header = header_snap[preg as usize];
        let at_backedge = backedge_snap[preg as usize];

        if at_header == at_backedge {
            continue;
        }

        // If header expects vreg V in preg P, but back-edge has something else:
        if let Some(header_vreg) = at_header {
            // Find where header_vreg is at the back-edge
            let backedge_home = (0..32u8).find(|&p| {
                backedge_snap[p as usize] == Some(header_vreg)
            });

            if let Some(src_preg) = backedge_home {
                // vreg is in a different register — emit Mv
                state.pinsts.push(PInst::Mv { dst: preg, src: src_preg });
            } else {
                // vreg is spilled at back-edge — emit reload
                if let Some(slot) = state.spill.has_slot(header_vreg) {
                    let offset = -((slot as i32 + 1) * 4);
                    state.pinsts.push(PInst::Lw {
                        dst: preg,
                        base: FP_REG,
                        offset,
                    });
                }
                // If no spill slot exists, the vreg was dead at the back-edge.
                // Header allocated it but it's not needed — no fixup required.
            }
        }
    }
}
```

### 3. Update liveness for Loop

In `liveness.rs`, implement Loop liveness:

```rust
Region::Loop { header, body } => {
    let header_liveness = analyze_liveness(tree, *header, vinsts, pool);
    let body_liveness = analyze_liveness(tree, *body, vinsts, pool);

    // Conservative: live_in is the union of header and body live_in sets
    let combined = header_liveness.live_in.union(&body_liveness.live_in);

    Liveness {
        live_in: combined,
        live_out: RegSet::new(),
    }
}
```

### 4. Remove `AllocError::UnsupportedControlFlow`

After implementing both IfThenElse and Loop, remove the
`UnsupportedControlFlow` variant from `AllocError` and its Display impl.
Also remove the `BrIf`/`Br` rejection from `process_inst` if not already done
in phase 1.

## Tests

- Unit test: simple loop (header + body with counter) — walk completes without error.
- Unit test: loop with a call in the body — clobber/spill/reload across iterations.
- Unit test: loop where a vreg changes register between iterations — fixup move emitted.
- Unit test: nested loop — inner loop within outer body.

## Validate

```bash
cargo test -p lpvm-native-fa
cargo check -p lpvm-native-fa
# Test with CLI:
cargo run -p lp-cli -- shader-rv32fa lp-shader/lps-filetests/filetests/lpvm/native/native-call-control-flow.glsl --show-region 2>&1
# Should compile all functions (branch_helper, call_in_if, loop_helper, call_in_loop)
```

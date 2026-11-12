# Phase 4: Allocator + Emitter — Loop

## Scope

Handle `Region::Loop { body, continuing, exit_label, continuing_label, ... }`
in the allocator. After this phase, shaders with loops compile and run.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Background

A loop region at the VInst level:

```
  Label(loop_header)
  [body region]
  Label(continuing)           // from Phase 1 lowering fix
  [continuing region — evaluates condition, increments]
  BrIfNot(cond, exit_label)   // or BrIfZero depending on polarity
  Br(loop_header)             // back-edge
  Label(exit_label)
```

The allocator walks backward:

1. The back-edge `Br(loop_header)` — allocator sees this as a branch, pool
   must be empty (spill-at-boundary).
2. The continuing region — walked backward, allocating the condition vreg
   and any increment operations.
3. `Label(continuing)` — no-op for allocator.
4. The body region — walked backward.
5. `Label(loop_header)` — no-op for allocator.

### Loop-carried values

Values defined before the loop and used inside it are "loop-carried." Under
the spill-at-boundary strategy:

- At the loop header boundary, all live values are in spill slots.
- Inside the loop body, values are reloaded from spill slots on first use.
- At the back-edge, all live values are spilled again.

This means loop-carried values are loaded/stored every iteration. This is the
simplest correct approach and matches regalloc2's boundary invariant.

### Break / Continue

- `break` → `Br(exit_label)` — the allocator doesn't need special handling;
  it's just a branch VInst. The pool is spilled before the break.
- `continue` → `Br(continuing_label)` — same, just branches to the continuing
  label. Pool is spilled at boundary.

Both are lowered as part of IfThenElse regions inside the loop body (e.g.,
`if (cond) break;`), so they are handled by the Phase 3 ITE walker.

## Implementation

### `walk.rs` — `walk_loop`

```rust
fn walk_loop(
    &mut self,
    body: RegionId,
    continuing: RegionId,
    exit_label: LabelId,
    continuing_label: LabelId,
) -> Result<(), AllocError> {
    // Backward order: back-edge → continuing → body

    // 1. Boundary spill at back-edge (pool should be clean, but ensure it)
    //    The back-edge Br is the last VInst of the continuing region.
    //    After walking it, spill everything before the loop header.
    
    // 2. Walk continuing region
    self.walk_region(continuing)?;
    
    // 3. Boundary spill between body and continuing
    let cont_first = self.region_first_inst(continuing);
    self.boundary_spill_all(cont_first);

    // 4. Walk body region
    self.walk_region(body)?;

    // 5. Boundary spill at loop header (before body starts)
    let body_first = self.region_first_inst(body);
    self.boundary_spill_all(body_first);

    Ok(())
}
```

### Back-edge handling

The `Br(loop_header)` VInst at the end of the continuing region is just a
regular VInst with no use/def operands. The allocator processes it as a no-op
(no allocation needed). The key invariant is that the pool is empty at both
ends of the loop boundary — which `boundary_spill_all` ensures.

After processing the continuing region backward, we spill all before the
body starts. After processing the body backward, we spill all before the
loop header label. This ensures the invariant: at every region boundary,
all live values are in spill slots.

### Emitter

Same as Phase 3: flat emission. The VInsts already contain:
- `Label(loop_header)` — recorded for offset
- `Br(loop_header)` — back-edge, patched to loop header offset
- `Label(continuing)` — recorded for offset
- `Br(continuing)` — continue target, patched
- `Label(exit_label)` — recorded for offset
- `Br(exit_label)` — break target, patched

The emitter's existing two-pass label resolution handles all of these.

### Tests

Unit tests in the builder:

1. **loop_simple**: `while(true) { a += 1; if (a > 10) break; }; ret a`.
   Verify: boundary spills at loop header and back-edge, reload inside body,
   correct allocs for the increment and comparison.

2. **loop_with_continue**: Loop with a `continue` path. Verify the branch to
   `continuing_label` works correctly.

3. **loop_live_across**: Value defined before loop, used inside and after.
   Verify: spilled at loop entry, reloaded in body each iteration, available
   after loop exit.

4. **loop_nested**: Nested loops. Verify recursive walk handles multiple
   nesting levels.

### GLSL filetests

After this phase, GLSL filetests with loops should compile. Run the full
filetest suite and compare pass/fail counts to baseline.

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native
# Full filetest run
cargo test -p lps-filetests -- rv32fa --nocapture 2>&1 | tail -5
```

Target: significant reduction in compile-fails (the 196 from baseline should
drop to near zero).

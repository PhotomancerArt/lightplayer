# Phase 3: Allocator + Emitter — IfThenElse

## Scope

Handle `Region::IfThenElse { cond, then_body, else_body, ... }` in the
allocator and emitter. After this phase, if-else shaders compile and run.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Background

An IfThenElse region has the following structure at the VInst level:

```
  [cond region — linear, computes condition]
  BrIfZero(cond_vreg, else_label)     // or BrIfNot
  [then_body region]
  Br(merge_label)
  Label(else_label)
  [else_body region]
  Label(merge_label)
```

The lowerer already produces these VInsts. The allocator must:
1. Walk the merge point backward (empty — it's a label).
2. Walk `else_body` backward with an empty pool.
3. Boundary-spill before else_body.
4. Walk `then_body` backward with an empty pool.
5. Boundary-spill before then_body.
6. Walk `cond` backward (it ends with the branch; allocate the cond_vreg).

The emitter:
1. Emits `cond` region VInsts.
2. Emits the `BrIfZero` branch (label resolved to byte offset).
3. Emits `then_body` VInsts + edits.
4. Emits `Br(merge_label)`.
5. Emits `Label(else_label)`.
6. Emits `else_body` VInsts + edits.
7. Emits `Label(merge_label)`.

Labels and branches are already VInsts. The emitter already handles
`VInst::Label` (recording offset) and `VInst::Br` / `VInst::BrIfZero`
(patching). The main new work is the allocator.

## Implementation

### `walk.rs` — `walk_ite`

```rust
fn walk_ite(
    &mut self,
    cond: RegionId,
    then_body: RegionId,
    else_body: RegionId,
    merge_label: LabelId,
    else_label: LabelId,
) -> Result<(), AllocError> {
    // 1. Walk else_body (backward: this comes after then_body in forward order)
    self.walk_region(else_body)?;

    // 2. Boundary spill after else_body (clear pool)
    let else_first = self.region_first_inst(else_body);
    self.boundary_spill_all(else_first);

    // 3. Walk then_body
    self.walk_region(then_body)?;

    // 4. Boundary spill after then_body (clear pool)
    let then_first = self.region_first_inst(then_body);
    self.boundary_spill_all(then_first);

    // 5. Walk cond (contains the BrIfZero; allocator allocates its operand)
    self.walk_region(cond)?;

    Ok(())
}
```

Key insight: we use `boundary_spill_all` (not liveness-filtered) at branch
points because both arms are possible execution paths. Every value in a
register must be spilled before the branch so both arms can reload on demand.

### `boundary_spill_all`

Variant of `boundary_spill` that spills everything in the pool, regardless of
liveness (conservative but correct):

```rust
fn boundary_spill_all(&mut self, anchor_idx: u16) {
    for preg in self.pool.occupied_pregs() {
        if let Some(vreg) = self.pool.vreg_in(preg) {
            let slot = self.spill.get_or_assign(vreg);
            self.edits.push((
                EditPoint::Before(anchor_idx),
                Edit::Move {
                    from: Alloc::Reg(preg),
                    to: Alloc::Stack(slot),
                },
            ));
        }
    }
    self.pool.clear();
}
```

Note: `boundary_spill_all` uses `EditPoint::Before` — edits go before the
first instruction of the arm, which in the forward stream means "store to
stack before entering this arm." Alternatively, `EditPoint::After(last_of_cond)`
for the spill before branches. Need to pick the right anchor consistently.

### Emitter changes

The emitter already walks VInsts linearly. It needs to handle the region tree
to know when to emit labels and branches. Two approaches:

**Option A: Keep flat emission.** Since labels and branches are already VInsts,
the existing linear emission handles them. The emitter just needs to do a
two-pass approach for label resolution (already implemented: first pass records
label offsets, second pass patches branches).

**Option B: Region-tree emission.** Walk the region tree in forward order,
emitting region-by-region.

**Decision**: Option A (keep flat emission). The VInsts already contain all
the branches and labels in the right order. The allocator's edits are indexed
by VInst position. The emitter just walks the flat list, applying edits at
each position. No structural changes to the emitter needed.

### Tests

Unit tests in the builder:

1. **ite_simple**: A diamond CFG: `if (cond) { a = 1 } else { a = 2 }; ret a`.
   Verify: both arms produce correct allocs, boundary spills appear before
   each arm, merge point correctly reloads.

2. **ite_live_across**: Value defined before the if, used after merge.
   Verify: value is spilled at boundary, reloaded after merge.

3. **ite_nested**: Nested if-else. Verify recursive walk works.

### GLSL filetests

After this phase, GLSL filetests with simple if-else should compile and
produce correct output. Check against existing filetests.

## Validate

```bash
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa
# Run GLSL filetests with if-else
cargo test -p lps-filetests -- rv32fa --nocapture 2>&1 | head -50
```

# Phase 1: Refactor Lowering (Region Labels, Pure Computation Bodies)

## Scope of phase

Refactor the Region enum and lowering logic to support the spill-at-boundary
architecture:

1. Add label fields to `Region::IfThenElse` and `Region::Loop`
2. Modify lowering so body regions contain only computation VInsts
3. The walker will emit control-flow PInsts (Labels, J, BrIf) instead of
   discovering them through VInsts

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Update `Region::IfThenElse` in `region.rs`

```rust
Region::IfThenElse {
    head: RegionId,
    then_body: RegionId,
    else_body: RegionId,
    else_label: LabelId,    // NEW: else branch label
    merge_label: LabelId,   // NEW: merge point label
}
```

### 2. Update `Region::Loop` in `region.rs`

```rust
Region::Loop {
    header: RegionId,
    body: RegionId,
    header_label: LabelId,  // NEW: loop header label
    exit_label: LabelId,    // NEW: loop exit label
}
```

### 3. Refactor IfThenElse lowering in `lower.rs`

For `else_is_empty` case:
- Don't emit `Br` VInst — the walker will emit `J` directly
- Don't emit `Label(merge)` VInst — the walker will emit `Label` directly
- Store `merge` label in the `Region::IfThenElse`
- `else_body` should be `REGION_ID_NONE` (empty else case)

For non-empty else case:
- Don't emit `Br` VInst
- Don't emit `Label(else_label)` or `Label(end_label)` VInsts
- Store both labels in the region
- `else_body` is just the else computation (lower_range result)

Current structure to change:
```rust
// OLD (emitting VInsts that walker will discover):
let br_start = self.out.len() as u16;
self.out.push(VInst::Br { target: merge, ... });  // REMOVE
let br_region = self.region_tree.push(Region::Linear { start: br_start, end: br_end });
let else_body = self.region_tree.push_seq(&[br_region]);  // REMOVE
self.out.push(VInst::Label(merge, ...));  // REMOVE

// NEW (walker emits control flow):
let else_body = REGION_ID_NONE;  // empty else
let merge = self.alloc_label();  // still allocate, but store in region
```

### 4. Refactor Loop lowering in `lower.rs`

- Don't emit entry `Br` VInst — walker will handle entry
- Don't emit `Label(header)`, `Label(continuing)`, `Label(exit)` VInsts
- Don't emit back-edge `Br` VInst — walker will emit `J`
- Store `header_label` and `exit_label` in `Region::Loop`
- `header` region contains just header computation
- `body` region contains body computation only

The `loop_regions` Vec already tracks header/backedge indices; update it to
also store label IDs for the walker.

### 5. Update VInst processing in `walk.rs`

Skip BrIf, Br, and Label VInsts inside IfThenElse/Loop walks. These should
never be encountered in body regions now, but add a check that returns early
(with a TODO comment explaining they're handled by the walker).

Actually, since we removed them from body regions, this shouldn't be needed.
Keep the existing handling for BrIf/Br/Label in process_inst for Linear/Seq
regions (still used for top-level code and nested Seq regions).

## Tests

- Unit test: `IfThenElse` region has correct labels after lowering
- Unit test: `Loop` region has correct labels after lowering
- Lowering test: `IfThenElse` body regions contain no Br/Label VInsts
- Lowering test: `Loop` body region contains no Br/Label VInsts

## Validate

```bash
cargo test -p lpvm-native -- lower
```

The refactoring should not break existing straight-line tests since those
use `Linear` and `Seq` regions, not `IfThenElse` or `Loop`.

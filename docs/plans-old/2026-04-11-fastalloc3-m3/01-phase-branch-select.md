# Phase 1: Branch Infrastructure + Select32

## Scope

Add label tracking and branch fixups to `Rv32Emitter`. Add `PInst::Label`.
Handle `BrIf`, `Br`, `Label`, `Select32` VInsts in `walk.rs` and `emit_vinst`.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Add `PInst::Label` to `rv32/inst.rs`

```rust
// Add to PInst enum:
Label { id: u32 },

// Add to mnemonic():
PInst::Label { .. } => "label",
```

### 2. Add label fixup system to `rv32/rv32_emit.rs`

Add label offset tracking and branch fixup to `Rv32Emitter`:

```rust
use alloc::collections::BTreeMap;

pub struct Rv32Emitter {
    code: Vec<u8>,
    relocs: Vec<PhysReloc>,
    label_offsets: BTreeMap<u32, usize>,  // label id → byte offset
    branch_fixups: Vec<BranchFixup>,
    jal_fixups: Vec<JalFixup>,
}

struct BranchFixup {
    instr_offset: usize,    // byte offset of the branch instruction
    label_id: u32,          // target label
    src1: u32,              // rs1 register
    is_beq: bool,           // true=beq, false=bne
}

struct JalFixup {
    instr_offset: usize,
    label_id: u32,
}
```

In `emit()`:
- `PInst::Label { id }`: record `label_offsets[id] = self.code.len()`, emit nothing.
- `PInst::Beq/Bne/J`: if the label offset is already known (backward branch),
  encode directly. Otherwise, emit a placeholder (0x00000000) and record a fixup.

Add `pub fn apply_fixups(&mut self)` that iterates fixups, computes
`label_offsets[id] - instr_offset` as the branch displacement, and patches the
encoded instruction bytes in `self.code`.

Add `pub fn finish_with_fixups(mut self) -> (Vec<u8>, Vec<PhysReloc>)` that
calls `apply_fixups()` before returning.

### 3. Handle BrIf, Br, Label in `walk.rs`

In `process_inst`, remove the rejection of `BrIf`, `Br`. Labels are already
skipped. Handle them:

```rust
VInst::BrIf { cond, target, invert, .. } => {
    // cond is a use
    let cond_preg = resolve_single_use(state, *cond, decision)?;
    if *invert {
        state.pinsts.push(PInst::Beq { src1: cond_preg, src2: 0, target: *target as u32 });
    } else {
        state.pinsts.push(PInst::Bne { src1: cond_preg, src2: 0, target: *target as u32 });
    }
    return Ok(());
}
VInst::Br { target, .. } => {
    state.pinsts.push(PInst::J { target: *target as u32 });
    return Ok(());
}
VInst::Label(id, _) => {
    state.pinsts.push(PInst::Label { id: *id as u32 });
    return Ok(());
}
```

Note: Labels now emit a `PInst::Label` instead of being silently skipped.
The label ID from `VInst::Label(LabelId, _)` maps directly to `PInst::Label { id }`.

### 4. Handle Select32 in `emit_vinst`

Select32 uses the same pattern as lpvm-native: `dst = (if_true - if_false) & cond + if_false`.

```rust
VInst::Select32 { .. } => {
    // uses: [cond, if_true, if_false], def: dst
    let (dst_p, cond_p, true_p, false_p) = (dst(), use_pregs[0], use_pregs[1], use_pregs[2]);
    Ok(vec![
        PInst::Sub { dst: SCRATCH, src1: true_p, src2: false_p },
        PInst::And { dst: SCRATCH, src1: SCRATCH, src2: cond_p },
        PInst::Add { dst: dst_p, src1: SCRATCH, src2: false_p },
    ])
}
```

Note: `for_each_use` on Select32 visits `cond, if_true, if_false` in that order.
Remove `VInst::Select32` from the rejection list in `process_inst`.

### 5. Update `emit_pinsts` caller

In `fa_alloc/mod.rs`, ensure `emit_pinsts` (or `Rv32Emitter`) calls
`apply_fixups()` before returning the code bytes. Update
`Rv32Emitter::finish_with_relocs` or add a new method that includes fixups.

## Tests

- Unit test: `PInst::Label` emits zero bytes but `apply_fixups` resolves a
  `PInst::J` target correctly.
- Unit test: `process_inst` handles `BrIf` without error.
- Unit test: `emit_vinst` for `Select32` produces 3 PInsts.
- Existing tests still pass (walk_region_allocates_simple, etc.).

## Validate

```bash
cargo test -p lpvm-native-fa
cargo check -p lpvm-native-fa
```

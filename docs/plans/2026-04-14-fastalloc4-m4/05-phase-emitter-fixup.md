# Phase 5: Emitter Label Resolution + Edit Anchoring

## Scope

Verify and fix the emitter's handling of:
1. Label resolution for branches across regions.
2. Edit anchoring at boundary points (edits from boundary_spill).
3. Branch offset patching for back-edges (negative offsets for loops).

This phase may be a no-op if flat emission already handles everything, but
it's called out explicitly because label resolution with backward branches
is a common source of bugs.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Potential Issues

### 1. Branch offset signedness

RISC-V `beq`/`bne`/`jal` use signed immediates for offsets. Forward branches
are positive, back-edges are negative. The emitter must handle negative offsets
for `Br(loop_header)` where the label offset is before the branch instruction.

Check: `rv32/emit.rs` branch encoding — does it handle negative offsets?
The RISC-V B-type immediate is sign-extended, so the encoding must preserve
the sign bit. The J-type (JAL) immediate is also sign-extended.

### 2. Edit placement at boundaries

Boundary spill edits use anchors like `EditPoint::Before(first_inst_of_region)`.
The emitter processes edits at each instruction index. Verify that:

- `EditPoint::Before(idx)` edits are emitted before instruction `idx`.
- `EditPoint::After(idx)` edits are emitted after instruction `idx`.
- Multiple edits at the same anchor are emitted in the correct order
  (stores before loads, etc.).

### 3. Two-pass label resolution

The emitter already does a two-pass approach:
1. First pass: emit all instructions, record label byte offsets.
2. Second pass: patch branch immediates with resolved offsets.

Verify this works for:
- Forward branches (if-else: `BrIfZero` to `else_label`).
- Forward branches (loop: `break` to `exit_label`).
- Backward branches (loop: `Br(loop_header)` back-edge).
- Forward branches (loop: `continue` to `continuing_label`).

### 4. Edit-injected instructions and label offsets

Boundary spill edits inject `sw`/`lw` instructions between VInsts. These
affect byte offsets for labels that come after the injection point. The
two-pass approach must account for this: labels are recorded during the first
pass which includes edit emission, so their offsets already include injected
instructions.

Verify: the first pass emits VInsts AND edit instructions, recording label
offsets during this combined pass.

## Implementation

Mostly verification and targeted fixes. Likely changes:

1. If branch encoding doesn't handle negative offsets, fix the B-type / J-type
   immediate encoding in `rv32/emit.rs`.
2. If edit ordering at boundaries is wrong, fix the edit sort/emit logic in
   `emit.rs`.
3. Add assertions in debug mode for:
   - All labels referenced by branches were recorded.
   - Branch offsets fit in immediate fields.

## Tests

1. **Branch offset test**: Unit test encoding a backward branch and verifying
   the binary output has correct negative offset.

2. **End-to-end GLSL filetests**: Run filetests that exercise both ITE and
   loops. If output is incorrect (wrong pixel values), add debug logging to
   trace edit emission order and label offsets.

## Validate

```bash
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa
# Full filetest suite
cargo test -p lps-filetests -- rv32fa
```

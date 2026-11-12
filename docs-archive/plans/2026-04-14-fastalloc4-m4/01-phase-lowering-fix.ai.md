# Phase 1: Lowering Fix — Emit Label(continuing)

## Scope

Fix the lowerer to emit `Label(continuing)` as a VInst in the loop body so
that `Br(continuing)` from `Continue` statements has a resolvable target.
Currently the lowerer defers this to "the walker" but neither walker nor
emitter produces it.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### `lower.rs` — LowerCtx::lower_range

In the `LpirOp::LoopStart` arm, the body is lowered as:

```rust
let body = self.lower_range(i + 1, eo)?;
```

The old backend (`lpvm-native`) emits `Label(continuing)` between the body and
continuing blocks:

```rust
if co < eo {
    self.out.push(VInst::Label(continuing, Some(*continuing_offset)));
    self.lower_range(co, eo.saturating_sub(1))?;
}
```

The FA lowerer should do the same. Before lowering the continuing range, emit
the label. The body should be split into two sub-ranges:

1. `lower_range(i + 1, co)` — main body (before continuing)
2. Emit `VInst::Label(continuing, Some(co as u32))`
3. `lower_range(co, eo)` — continuing block (condition + increment)

These sub-ranges can be combined into a Seq region, or left as part of a single
body region. The simplest approach: keep `lower_range(i + 1, eo)` as one call
but have the `BrIfNot` / `Continue` / `Break` handling work within it. The
`Label(continuing)` needs to appear at the VInst position corresponding to `co`.

Concretely, in the `LoopStart` arm:

```rust
// Body: main body from after LoopStart to continuing_offset
let co = *continuing_offset as usize;
let eo = *end_offset as usize;

self.loop_stack.push(LoopFrame { continuing, exit });

let body = if co < eo && co > i + 1 {
    // Body has both main and continuing sections
    let main_body = self.lower_range(i + 1, co)?;
    self.out.push(VInst::Label(continuing, pack_src_op(Some(co as u32))));
    let cont_body = self.lower_range(co, eo)?;
    // Combine into Seq
    self.region_tree.push_seq(&[main_body, cont_body])
} else if co < eo {
    // No main body, just continuing
    self.out.push(VInst::Label(continuing, pack_src_op(Some(co as u32))));
    self.lower_range(co, eo)?
} else {
    // Body only, no continuing block
    self.lower_range(i + 1, eo)?
};
```

Note: The `Label(continuing)` VInst sits between the main body and continuing
regions. Since it's just a label marker, the allocator treats it as a no-op
(it has no def/use operands). The emitter records it for branch fixups.

### Test

Verify that a shader with `continue` compiles. The simplest check is a unit
test in `lower.rs` that lowers a loop with a continuing block and asserts the
`Label(continuing)` VInst appears in the output.

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native
```

Existing tests must still pass. The `UnsupportedControlFlow` error will still
fire at allocate-time (we haven't added region walking yet), but lowering
should succeed.

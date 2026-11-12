# Phase 2: Control Product Model

## Scope Of Phase

Add the graph-level `ControlProduct` model and slot/value support.

In scope:

- Add `ControlProduct` to `lpc-model`.
- Add `ControlExtent` with neutral axis names.
- Add `ControlProduct` support to `LpValue`.
- Add a semantic control product slot type.
- Document that native LP control samples are `unorm16`.

Out of scope:

- Engine dispatch for control rendering.
- Fixture/output behavior changes.
- Protocol-specific E1.31/Art-Net packing.

## Code Organization Reminders

- Put `ControlProduct` in its own file.
- Keep semantic slot leaves in `slot/slots` or `slots` according to nearby
  conventions.
- Keep docs close to the public types.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/resource/mod.rs`
- `lp-core/lpc-model/src/resource/control_product.rs`
- `lp-core/lpc-model/src/value/lp_value.rs`
- `lp-core/lpc-model/src/value/lp_type.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`
- `lp-core/lpc-model/src/slots/mod.rs`
- `lp-core/lpc-model/src/slots/control_product.rs`

Expected shape:

```rust
pub struct ControlProduct {
    pub node: NodeId,
    pub output: u32,
    pub preferred_extent: ControlExtent,
}

pub struct ControlExtent {
    pub rows: u32,
    pub samples_per_row: u32,
}
```

Constraints:

- Avoid names like `universe` in core model names.
- Do not encode DMX512 assumptions in `ControlExtent`.
- Add basic serde/schema support matching adjacent model types.

## Validate

```bash
cargo test -p lpc-model
cargo check -p lpc-model --features schema-gen
```

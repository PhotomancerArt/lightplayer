# Phase 1: Model Binding Vocabulary

## Scope Of Phase

Add the shared binding vocabulary to `lpc-model`.

In scope:

- Add model-level binding modules and exports.
- Add semantic parsed endpoint types for bus-slot and node-slot refs.
- Add `BindingDef` and `BindingDefs`.
- Add serde support for the intended TOML shape.
- Add structural validation for binding entries.

Out of scope:

- Updating source node defs to use bindings.
- Updating examples.
- Runtime loader integration.
- Deleting `lpc-source/src/prop`.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `mod.rs` focused on module declarations and re-exports.
- Put tests at the bottom of each file.
- Keep binding concepts in `lpc-model`; do not add new binding vocabulary under
  `lpc-source/src/prop`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/slot/slot_path.rs`
- `lp-core/lpc-model/src/node/relative_node_ref.rs` or equivalent current file
- `lp-core/lpc-model/src/bus/*` if bus channel naming helpers already exist

Add:

```text
lp-core/lpc-model/src/binding/
  mod.rs
  binding_def.rs
  binding_defs.rs
  binding_endpoint.rs
  bus_slot_ref.rs
  node_slot_ref.rs
```

Expected concepts:

- `NodeSlotRef`
  - semantic parsed form of `..shader#output`
  - fields should be parsed Rust values, likely:
    - `node: RelativeNodeRef`
    - `slot: SlotPath`
  - serde should accept/emit the compact string form.

- `BusSlotRef`
  - semantic parsed form of `bus#visual.out`
  - fields should be parsed Rust values, likely:
    - `slot: SlotPath`
  - serde should accept/emit the compact string form.
  - Do not over-design bus resolution in this phase.

- `BindingEndpoint`
  - variants:
    - `Bus(BusSlotRef)`
    - `Node(NodeSlotRef)`
    - `Literal(LpValue)`
  - serde should support compact string endpoints for bus/node refs.
  - literal shape may use an explicit table/value form if needed; keep it simple.

- `BindingDef`
  - fields:
    - `source: Option<BindingEndpoint>`
    - `target: Option<BindingEndpoint>`
  - validation method should enforce exactly one of `source` or `target`.
  - validation should reject `target = Literal(...)`.

- `BindingDefs`
  - stable-key map from slot name to `BindingDef`.
  - use existing slot/map wrapper patterns where they fit.
  - provide default-empty and `is_empty`.

Tests to add:

- `NodeSlotRef` parses and round-trips `..shader#output`.
- `BusSlotRef` parses and round-trips `bus#visual.out`.
- `BindingDef` accepts source-only and target-only.
- `BindingDef` rejects both source and target.
- `BindingDef` rejects neither source nor target.
- `BindingDef` rejects literal target.
- `BindingDefs` TOML round-trips:

  ```toml
  [bindings.output]
  target = "bus#visual.out"
  ```

## Validate

Run:

```bash
cargo fmt --package lpc-model
cargo test -p lpc-model
cargo check -p lpc-model --features schema-gen
```

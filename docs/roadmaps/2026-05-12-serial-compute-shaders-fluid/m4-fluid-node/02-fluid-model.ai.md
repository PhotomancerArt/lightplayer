# Phase 2: Fluid Model

## Scope Of Phase

Add `FluidDef`, `FluidState`, and wire them into the model-level node definitions.

In scope:

- Add `FluidDef`.
- Add `FluidState`.
- Add `NodeDef::Fluid`.
- Add `NodeKind::Fluid`.
- Register/export model types.
- Add TOML parsing and shape tests.

Out of scope:

- Engine runtime node.
- Solver movement.
- Example project updates.

## Code Organization Reminders

- Put each fluid concept in its own file:
  - `fluid_def.rs`
  - `fluid_state.rs`
  - existing `fluid_emitter.rs`
- Keep `mod.rs` to declarations and re-exports.
- Tests go at the bottom of the file they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/fluid/mod.rs`
- `lp-core/lpc-model/src/nodes/fluid/fluid_emitter.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/node/kind.rs`
- `lp-core/lpc-model/src/lib.rs`

Create `FluidDef`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root, view)]
pub struct FluidDef {
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,

    #[serde(default = "default_size")]
    pub size: Dim2uSlot,
    #[serde(default = "default_solver_iterations")]
    pub solver_iterations: ValueSlot<u32>,
    #[serde(default = "default_step_hz")]
    pub step_hz: PositiveF32Slot,
    #[serde(default = "default_fade_speed")]
    pub fade_speed: RatioSlot,
    #[serde(default = "default_viscosity")]
    pub viscosity: PositiveF32Slot,

    #[slot(consumed, merge = "by_key")]
    #[slot(map(key = "u32", value_ref = "lp::fluid::Emitter"))]
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub emitters: MapSlot<u32, FluidEmitter>,
}
```

Suggested defaults:

- size: `20x20`
- solver iterations: `3`
- step hz: `25.0`
- fade speed: `0.1`
- viscosity: `0.00003`

Create `FluidState`:

```rust
#[derive(lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct FluidState {
    #[slot(produced)]
    pub output: VisualProductSlot,
}
```

Add model tests:

- `kind = "fluid"` parses as `NodeDef::Fluid`.
- `FluidDef` shape includes `emitters` with consumed/by_key semantics.
- `FluidDef` can parse authored inline emitters.
- `FluidState` output has produced semantics.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model fluid
cargo check -p lpc-model --features schema-gen
```


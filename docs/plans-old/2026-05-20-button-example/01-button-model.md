# Phase 1: Button Model

## Scope Of Phase

Add the authored and runtime-state model vocabulary for a first-class `Button` node.

In scope:

- `ButtonDef` with endpoint/id/debounce/bindings.
- `ButtonState` with `down`, `held`, and `up` maps.
- `NodeKind::Button` and `NodeDef::Button`.
- Static shape/view generation fallout.
- Model serde and shape tests.

Out of scope:

- Runtime polling.
- Engine service plumbing.
- ESP32 HAL changes.
- Example shader behavior.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Use `lp-core/lpc-model/src/nodes/button/button_def.rs` for `ButtonDef`.
- Use `lp-core/lpc-model/src/nodes/button/button_state.rs` for `ButtonState`.
- Keep `button/mod.rs` as declarations/re-exports only.
- Put tests at the bottom of the file they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Add `lp-core/lpc-model/src/nodes/button/mod.rs`.

   Re-export:

   - `ButtonDef`
   - `ButtonDefView`
   - `ButtonState`
   - `ButtonStateView`

2. Add `lp-core/lpc-model/src/nodes/button/button_def.rs`.

   Suggested shape:

   ```rust
   use serde::{Deserialize, Serialize};

   use crate::{BindingDefs, HardwareEndpointSpec, Slotted, ValueSlot};

   pub const DEFAULT_BUTTON_ENDPOINT_SPEC: &str = "button:gpio:D9";
   pub const DEFAULT_BUTTON_ID: u32 = 1;
   pub const DEFAULT_BUTTON_STABLE_MS: u64 = 30;

   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Slotted)]
   pub struct ButtonDef {
       #[serde(default = "default_endpoint_slot")]
       pub endpoint: ValueSlot<HardwareEndpointSpec>,
       #[serde(default = "default_id_slot")]
       pub id: ValueSlot<u32>,
       #[serde(default = "default_stable_ms_slot")]
       pub stable_ms: ValueSlot<u64>,
       #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
       pub bindings: BindingDefs,
   }
   ```

   Keep the dependency direction clean: do not import `lpc-shared::hardware::ButtonDebouncer` into
   `lpc-model` for the default value.

3. Add `lp-core/lpc-model/src/nodes/button/button_state.rs`.

   Suggested shape:

   ```rust
   use crate::{ControlMessage, MapSlot, Slotted};

   #[derive(Debug, Clone, Default, PartialEq, Slotted)]
   pub struct ButtonState {
       pub down: MapSlot<u32, ControlMessage>,
       pub held: MapSlot<u32, ControlMessage>,
       pub up: MapSlot<u32, ControlMessage>,
   }
   ```

   If `MapSlot<u32, ControlMessage>` needs explicit constructors or trait bounds, follow existing
   `FluidState` / compute-state patterns rather than inventing a new container.

4. Wire model exports:

   - `lp-core/lpc-model/src/nodes/mod.rs`
   - `lp-core/lpc-model/src/lib.rs`
   - `lp-core/lpc-model/src/node/kind.rs`
   - `lp-core/lpc-model/src/nodes/node_def.rs`

   Update all match arms for:

   - `kind`
   - `kind_name`
   - `variant_name`
   - `SlotAccess`
   - `SlotMutAccess`
   - any `as_*` accessors where local pattern warrants it.

   Add constants:

   - `BUTTON_VARIANT: &str = "Button"`
   - include it in `NODE_DEF_VARIANT_NAMES`.

5. Add tests:

   - `ButtonDef` parses minimal TOML:

     ```toml
     kind = "Button"
     ```

     Assert endpoint defaults to `button:gpio:D9`, id defaults to `1`, and stable debounce is
     non-zero.

   - `ButtonDef` parses explicit endpoint:

     ```toml
     kind = "Button"
     endpoint = "button:gpio:D9"
     id = 12
     stable_ms = 25

     [bindings.held]
     target = "bus#trigger"
     ```

   - Generated `ButtonDefView` and `ButtonStateView` compile against a registry where their static
     shapes are registered.

## Validate

Run:

```bash
cargo fmt --check
cargo test -p lpc-model button
cargo check -p lpc-model --no-default-features
```


# Button Example Design

## Scope Of Work

Build the first authored button-node slice and a checked-in `examples/button` project. A button
wired from XIAO ESP32-C6 D9 to GND should be configured as an active-low input with internal pull-up.
The runtime node should expose three graph-level outputs:

- `down`: one-frame trigger on debounced press.
- `held`: trigger/state message present every frame while pressed.
- `up`: one-frame trigger on debounced release.

The example uses only `held` and renders a circle in a basic shader while the button is down.

Radio nodes, playlist behavior, OSC-style addresses/args, and richer button gestures are out of
scope.

## File Structure

```text
lp-core/lpc-model/src/
  nodes/
    button/
      button_def.rs
      button_state.rs
      mod.rs
    mod.rs
    node_def.rs
  node/kind.rs

lp-core/lpc-engine/src/
  engine/
    engine_services.rs
    project_loader.rs
  node/
    contexts.rs
  nodes/
    button/
      button_node.rs
      mod.rs
    mod.rs

lp-core/lpc-shared/src/hardware/
  hardware_system.rs
  virtual_button_driver.rs

lp-app/lpa-server/src/
  project.rs
  project_manager.rs
  server.rs

lp-fw/fw-esp32/src/
  board/esp32c6/init.rs
  hardware/button.rs
  main.rs

lp-fw/fw-emu/src/
  main.rs

examples/button/
  project.toml
  button.toml
  shader.toml
  shader.glsl
  fixture.toml
  output.toml
  clock.toml
```

## Architecture Summary

The button node is an authored runtime node, not a firmware special case:

```text
HardwareSystem
  opens button:gpio:D9
        |
        v
EngineServices button service
        |
        v
ButtonNode ticks and polls ButtonInput
        |
        +--> down: Map<u32, ControlMessage>
        +--> held: Map<u32, ControlMessage> --> bus#trigger --> Shader consumed events
        +--> up:   Map<u32, ControlMessage>
```

`ButtonDef` should be ordinary slotted model data:

```rust
pub struct ButtonDef {
    pub endpoint: ValueSlot<HardwareEndpointSpec>,
    pub id: ValueSlot<u32>,
    pub stable_ms: ValueSlot<u64>,
    pub bindings: BindingDefs,
}
```

Default endpoint can be `button:gpio:D9` for this first slice because the immediate example is the
driving use case. `id` defaults to `1`. `stable_ms` defaults to `ButtonDebouncer::DEFAULT_STABLE_MS`
or an equivalent model-side constant if importing the shared hardware type into `lpc-model` would
create the wrong dependency direction.

`ButtonState` should be runtime-produced state with three fields:

```rust
pub struct ButtonState {
    pub down: MapSlot<u32, ControlMessage>,
    pub held: MapSlot<u32, ControlMessage>,
    pub up: MapSlot<u32, ControlMessage>,
}
```

Each output field uses `SlotSemantics::produced()` and shape-compatible `MapSlot<u32,
ControlMessage>`. Empty map means "no message this frame". On a debounced press edge:

- increment the node's internal sequence counter;
- set `down = { id: ControlMessage { id, seq } }`;
- set `held = { id: ControlMessage { id, seq } }`;
- set `up = {}`.

While still held:

- `down = {}`;
- `held = { id: ControlMessage { id, seq } }`;
- `up = {}`.

On release:

- increment sequence;
- `down = {}`;
- `held = {}`;
- `up = { id: ControlMessage { id, seq } }`.

When idle:

- all three maps are empty.

## Main Components And Interactions

### Button Model

Add `ButtonDef` and `ButtonState` under `lpc-model/src/nodes/button/`, then wire them into the
closed model enum:

- `NodeKind::Button`
- `NodeDef::Button(ButtonDef)`
- kind string/variant name `Button`
- `NodeDef::as_button`
- `nodes::button` module exports
- root `lpc_model` re-exports

The existing `lpc-model` build script should generate static shapes and views once the source files
are part of the module tree.

### Button Services

Expose a narrow button-opening surface through engine services. Prefer a small engine-owned wrapper
instead of putting `HardwareSystem` directly on every node:

```rust
pub trait ButtonService {
    fn open_button_by_spec(
        &self,
        endpoint: &HardwareEndpointSpec,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError>;
}
```

`EngineServices` can hold `Option<Rc<dyn ButtonService>>`, and `TickContext` can expose a borrowed
service to runtime nodes. `HardwareSystem` can either implement this trait directly or be wrapped by
a newtype if orphan/object-shape constraints make that cleaner.

Server-side constructors should pass the same root hardware system used for output into
`EngineServices` so normal loaded projects can use button nodes.

### Runtime Button Node

`ButtonNode` owns:

- `ButtonDefView` cache for reading authored endpoint/id/stable_ms updates;
- optional opened `Box<dyn ButtonInput>`;
- current endpoint/config identity so it can reopen if authored config changes;
- `ButtonState`;
- internal `seq`;
- current stable `is_held` state;
- edge flags for the current frame.

On tick:

1. Read authored endpoint/id/stable_ms through the generated view.
2. Open/reopen the button input through `ctx.button_service()` if needed.
3. Poll with `ctx.now_ms().unwrap_or_else(...)`.
4. Convert `ButtonEventKind::Pressed`/`Released` into `down`/`up` edges and update held state.
5. Publish runtime state maps for the current frame.

If no button service is installed, the node should report a clear `NodeError` rather than silently
pretending the button is unpressed.

### Virtual And ESP32 Hardware

For host tests, keep using `VirtualButtonDriver`. Add a shared handle or helper so tests can set the
pressed state for `D9`/`/gpio/20` after constructing the same hardware system used by the engine.

For ESP32, update the owned peripheral path to support D9/GPIO20:

- return `GPIO20` from `init_board()`;
- add an ESP32 button driver/input constructor for GPIO20 using internal pull-up;
- register that button driver in normal firmware `HardwareSystem`;
- keep the existing `test_button` GPIO4 diagnostic working unless intentionally moved in a later
  change.

The first ESP32 driver may be D9-specific. A generic manifest-dispatch table is useful later but
larger than this example slice.

### Example Project

`examples/button/button.toml` should look roughly like:

```toml
kind = "Button"
endpoint = "button:gpio:D9"
id = 1

[bindings.held]
target = "bus#trigger"
```

`shader.toml` consumes `bus#trigger` as a sentinel map:

```toml
[bindings.events]
source = "bus#trigger"

[consumed_slots.events]
kind = "map"
key = "u32"
value = "lp::control::Message"
mapping = { kind = "sentinel", len = 1, key = "id", empty_key = 0 }
```

`shader.glsl` should render a simple background and draw a circle when `events[0].id != 0u`.

Tests should load the example, inject virtual D9 pressed/released state, tick frames, render the
shader, and assert the circle appears only while held.


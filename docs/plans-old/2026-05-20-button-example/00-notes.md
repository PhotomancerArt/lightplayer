# Button Example Plan Notes

## Scope Of Work

Create `examples/button`, a concrete project where a physical GPIO button drives a shader-visible
control message. The first behavior is intentionally small: wire a normally-open button from D9 to
GND, configure the input with an internal pull-up, expose button state through an authored button
node, bind the `held` output to `bus#trigger`, and render a circle in a basic shader while the
button is held.

In scope:

- Add an authored `Button` node definition in `lpc-model`.
- Add a runtime `ButtonNode` in `lpc-engine`.
- Expose button hardware through `EngineServices` / `TickContext` without making nodes own global
  hardware directly.
- Add button output slots for edge/state messages.
- Add `examples/button` with `button.toml`, shader, fixture, output, clock, and project files.
- Validate the example on host/emu paths and keep the ESP32 firmware build path ready for D9.

Out of scope:

- Radio nodes.
- Playlist / visual switching.
- OSC address/args payloads.
- Rich button modes such as long-press, double-click, repeat rate, or latching.
- Feeding raw hardware-specific `ButtonEvent` into shaders.

## Current Codebase State

- The hardware button substrate already exists in `lpc-shared::hardware`:
  - `ButtonConfig` carries debounce stable time.
  - `ButtonInput::poll(now_ms)` returns debounced `ButtonEvent`.
  - `ButtonEventKind` currently has hardware-level `Pressed` and `Released`.
  - `VirtualButtonDriver` can inject pressed state for tests and exposes endpoints as
    `button:gpio:<display-label>`.
- The ESP32 diagnostic button path already uses internal pull-up and active-low wiring:
  - `fw-esp32/src/hardware/button.rs` creates `Input::new(pin, InputConfig::default().with_pull(Pull::Up))`.
  - `poll` treats `input.is_low()` as pressed.
  - The diagnostic is currently hardwired to owned `GPIO4`.
- The Seeed XIAO ESP32-C6 manifest includes D9:
  - `D9` maps to `/gpio/20`.
  - `/gpio/20` supports both `gpio-output` and `gpio-input`.
  - The example should therefore author as `button:gpio:D9`.
- Engine runtime nodes currently tick via `NodeRuntime::tick(&mut TickContext)`.
  - `TickContext` exposes resolver/time/graphics/runtime-buffer services.
  - It does not expose `HardwareSystem`, `ButtonInput`, or button-specific services yet.
- `EngineServices` currently owns:
  - project root identity,
  - optional `OutputProvider`,
  - optional `TimeProvider`,
  - output sink flush state.
  It does not yet own or pass through a root hardware system.
- Server construction currently passes an `OutputProvider` into each project, not a full
  `HardwareSystem`.
  - `fw-esp32` creates a root `HardwareSystem` for output and radio, but only wraps output in
    `Esp32OutputProvider` before handing it to `LpServer`.
  - `fw-emu` also creates `HardwareSystem::with_virtual_drivers`, but only passes the output
    provider into `LpServer`.
- The event/control message slice is complete enough for this example:
  - `ControlMessage { id: u32, seq: u32 }` exists.
  - `TriggerEvent` is a semantic alias.
  - Shader consumed slots can read sentinel maps of `lp::control::Message`.
  - `examples/events` proves `bus#trigger` can flow into a visual shader.
- The authored node enum is closed. Adding `Button` requires updating:
  - `lpc-model::NodeKind`,
  - `lpc-model::NodeDef`,
  - `lpc-model::nodes` exports,
  - project loader runtime attachment and binding registration,
  - generated slot shape/view code through the existing `lpc-model` build script.

## User Notes

- The user will wire a button from D9 to GND because that is where the button is soldered and it is
  ergonomic.
- Use an internal pull-up resistor if available. It is available on the ESP32 diagnostic path.
- The example project should be named `examples/button`.
- The first shader behavior should be simple: pressing/holding the button turns on a circle.
- The button module should probably expose three outputs for down/held/up.
- Use the `held` message in this first example.

## Suggested Event Slot Names

Suggested produced slot names:

- `down`: one-frame edge message when the debounced button transitions to pressed.
- `held`: state-like message present every frame while the debounced button is pressed.
- `up`: one-frame edge message when the debounced button transitions to released.

Reasoning:

- These names are short and read cleanly in TOML:
  - `[bindings.held] target = "bus#trigger"`
- They avoid overloading `pressed`, which can mean either the edge or the current stable state.
- If later we need booleans, names such as `is_down` or `pressed` can be reserved for value state,
  while `down` and `up` remain edge outputs.

The output type should be `MapSlot<u32, ControlMessage>` for all three slots, matching
`examples/events` and sentinel shader input support. An empty map means no message. A non-empty map
contains one message keyed by the button's configured `id`.

## Open Questions

### Q1. Should the first authored button endpoint be `button:gpio:D9` even though current ESP32 code only owns GPIO4?

Suggested answer: yes. Use `button:gpio:D9` in `examples/button`, and include the ESP32 plumbing to
open D9 by extending the board/peripheral ownership path for `GPIO20`.

Context: the manifest already says D9 is `/gpio/20`, but `init_board()` currently returns only
`GPIO18`, `GPIO4`, and `WIFI` among the GPIO-ish resources used by runtime code. If we keep the
button node host-only or GPIO4-only, the example will not match the physical wiring request.

User answer: yes, use D9. The user soldered the button there.

### Q2. What should `held` do with sequence numbers?

Suggested answer: `seq` increments on debounced transitions, not every frame. While held, the
`held` map contains the same `ControlMessage { id, seq }` each frame until release. This makes
`held` useful as shader-visible state while preserving `seq` as retrigger identity for edge users.

Context: `down` and `up` are true edge outputs. `held` is the one state-like output in this slice,
because the example wants a circle while the button is down.

User answer: yes.

### Q3. Should the button node publish exactly one message id?

Suggested answer: yes. Add an authored `id: u32` field on `ButtonDef`, defaulting to `1`. The node
uses that id as the map key for `down`, `held`, and `up`.

Context: `ControlMessage` intentionally has only `id` and `seq` today. This example does not need
source strings or OSC addresses, and shader sentinel maps need a stable key field.

User answer: yes.

### Q4. Should host tests use injected virtual button state?

Suggested answer: yes. Add a tiny engine-facing button service wrapper around `HardwareSystem` so
tests can install a virtual system and set D9 pressed/released. Avoid making tests depend on ESP32
HAL code.

Context: `VirtualButtonDriver` already supports `set_pressed`, but that handle is hidden once it is
boxed inside `HardwareSystem::with_virtual_drivers`. The plan may need a small test-only or shared
handle shape so engine tests can drive virtual button state.

User answer: yes.

### Q5. Does this plan update `LpServer::new` to accept full hardware services?

Suggested answer: do the smallest useful version. Add optional button/hardware service plumbing to
`EngineServices` and route it through `Project::new` / `ProjectManager::load_project` /
`LpServer::new` only if needed for firmware examples to run as normal loaded projects. If that
surface gets too wide, introduce an `EngineServiceProvider` struct rather than adding many positional
constructor args.

Context: output/time already thread through `LpServer` into `EngineServices`; button nodes need a
similar path or they cannot run outside direct `ProjectLoader` tests.

User answer: yes.

### Q6. Are `down`, `held`, and `up` acceptable output names?

Suggested answer: yes.

Context: `down` is the debounced press edge, `held` is the stable pressed state, and `up` is the
debounced release edge. This keeps `pressed` available later for a boolean value if needed.

User answer: yes.

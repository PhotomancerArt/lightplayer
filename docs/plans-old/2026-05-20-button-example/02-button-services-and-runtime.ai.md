# Phase 2: Button Services And Runtime

## Scope Of Phase

Implement the runtime button node and the service plumbing required for loaded projects to open
button inputs.

In scope:

- Engine-facing button service.
- `EngineServices` / `TickContext` access to the service.
- `ButtonNode` runtime.
- `ProjectLoader` support for authored `Button` nodes and `down`/`held`/`up` target bindings.
- Host tests using virtual button injection.

Out of scope:

- ESP32 D9 HAL plumbing.
- Checked-in `examples/button` files.
- Radio or playlist behavior.

## Code Organization Reminders

- Put runtime code under `lp-core/lpc-engine/src/nodes/button/button_node.rs`.
- Keep `nodes/button/mod.rs` small.
- Keep service abstractions near `engine_services.rs` unless a separate file becomes obviously
  clearer.
- Tests go at file bottoms.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Add a narrow button service surface.

   Prefer one of these shapes:

   ```rust
   pub trait ButtonService {
       fn open_button_by_spec(
           &self,
           endpoint: &HardwareEndpointSpec,
           config: ButtonConfig,
       ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError>;
   }
   ```

   Or a local wrapper type if using `Rc<HardwareSystem>` directly is simpler. The runtime node
   should not need to know about output/radio methods.

2. Extend `EngineServices`.

   Add:

   - `button_service: Option<Rc<dyn ButtonService>>`
   - `set_button_service`
   - `button_service`

   Update `TickContext::with_render_services` or its replacement constructor to carry an optional
   `Rc<dyn ButtonService>`. Expose `ctx.button_service()`.

   Keep existing callers compiling. Default should be `None`.

3. Route services through engine ticking.

   Find where `TickContext` is created in `lp-core/lpc-engine/src/engine/engine.rs` and pass the
   `EngineServices` button service into tick contexts. Preserve graphics/time behavior.

4. Add runtime module exports:

   - `lp-core/lpc-engine/src/nodes/button/mod.rs`
   - update `lp-core/lpc-engine/src/nodes/mod.rs`.

5. Implement `ButtonNode`.

   Suggested fields:

   - `state: ButtonState`
   - `def_view: Option<ButtonDefView>`
   - `input: Option<Box<dyn ButtonInput>>`
   - `opened_endpoint: Option<HardwareEndpointSpec>`
   - `opened_stable_ms: Option<u64>`
   - `seq: u32`
   - `held: bool`
   - `current_message_seq: u32`

   Tick algorithm:

   - Read `endpoint`, `id`, and `stable_ms` from authored def through `ButtonDefView`.
   - Reopen input when endpoint or stable debounce changes.
   - Use `ctx.now_ms()` for polling. If no time provider exists, use `u64::from(ctx.revision().as_u32_or_equivalent)` or add a tiny monotonic fallback local to the node; document whichever existing revision accessor is actually available.
   - Convert `ButtonEventKind::Pressed`:
     - increment `seq`;
     - `held = true`;
     - publish `down` and `held`.
   - Convert `ButtonEventKind::Released`:
     - increment `seq`;
     - `held = false`;
     - publish `up`.
   - With no edge:
     - publish `held` only if `held == true`.

   Use `ControlMessage::new(id, seq)` and `MapSlot<u32, ControlMessage>` data. If direct
   `MapSlot` mutation is awkward, construct `SlotData::Map(SlotMapDyn { ... })` for the state
   update, but prefer typed model slots if available.

6. Runtime state shape registration:

   - `runtime_state_slots` returns `Some(&self.state)`.
   - `register_runtime_state_shapes` registers `ButtonState` and `ControlMessage`.

7. Update `ProjectLoader::attach_loaded_nodes`.

   Add a pass for `NodeDef::Button`.

   - Attach `ButtonNode`.
   - Register target bindings for `down`, `held`, and `up`.

8. Host tests.

   Add tests that:

   - Construct a virtual hardware system with a controllable `VirtualButtonDriver`.
   - Install it as the engine button service.
   - Load a small button project.
   - Bind `held` to `bus#trigger`.
   - Drive D9/GPIO20 pressed and released.
   - Assert `held` resolves to a non-empty map while pressed and an empty map after release.

   If `VirtualButtonDriver::set_pressed` is inaccessible after boxing, add a small shared helper
   such as `VirtualButtonHandle` or a constructor that returns `(HardwareSystem, VirtualButtonDriverHandle)`.

## Validate

Run:

```bash
cargo fmt --check
cargo test -p lpc-engine button
cargo test -p lpc-engine engine_services
cargo test -p lpc-engine --test runtime_spine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```


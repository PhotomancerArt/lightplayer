# M3 Radio Node Design

## Scope Of Work

Add an authored bidirectional radio event node for the first fyeah-sign wireless trigger path. The
node sends local `ControlMessage` trigger maps over the existing radio hardware API and publishes
accepted local plus deduped remote `ControlMessage` trigger maps to graph consumers.

The first reliability policy is repeated broadcast plus receiver-side dedupe. No acknowledgements,
ownership, TTL, routing, or state synchronization are included.

## File Structure

```text
lp-core/lpc-model/src/nodes/
  mod.rs
  node_def.rs
  radio/
    mod.rs
    control_radio_def.rs

lp-core/lpc-model/src/node/
  kind.rs

lp-core/lpc-shared/src/hardware/
  radio_message.rs
  hardware_system.rs

lp-core/lpc-engine/src/engine/
  engine.rs
  engine_services.rs
  project_loader.rs

lp-core/lpc-engine/src/node/
  contexts.rs

lp-core/lpc-engine/src/nodes/
  mod.rs
  radio/
    mod.rs
    control_radio_node.rs

lp-app/lpa-server/src/
  project.rs

examples/
  button-radio-playlist/
    project.toml
    button.toml
    radio.toml
    playlist.toml
    ...
```

If M2 lands an `examples/button-playlist` directory before this phase executes, prefer extending or
copying that example instead of inventing a conflicting playlist example shape.

## Architecture Summary

`ControlRadioNode` is a small bridge between graph events and the existing hardware radio device:

```text
button.down ---------> bus#trigger -----> radio.input
                         ^                 |
                         |                 v
playlist.trigger <-------+---- radio.output <----- ESP-NOW / virtual radio
```

The local button event enters `bus#trigger`. `ControlRadioNode` consumes that same bus, accepts local
messages for repeated radio broadcast, and publishes accepted local plus remote messages through
`output` back to `bus#trigger`. Playlist consumes the same bus. The radio node never rebroadcasts
its own produced output.

## Model Shape

Add `ControlRadioDef` and `ControlRadioState`.

```rust
pub struct ControlRadioDef {
    pub bindings: BindingDefs,
    pub endpoint: ValueSlot<HardwareEndpointSpec>,
    pub channel: ValueSlot<u32>,
    pub repeat_count: ValueSlot<u32>,
    pub wifi_channel: OptionSlot<ValueSlot<u32>>,
}

pub struct ControlRadioState {
    #[slot(consumed, map(key = "u32", value_ref = "lp::control::Message"))]
    pub input: MapSlot<u32, ControlMessage>,

    #[slot(produced, map(key = "u32", value_ref = "lp::control::Message"))]
    pub output: MapSlot<u32, ControlMessage>,
}
```

Notes:

- `endpoint` defaults to `radio:espnow:0`.
- `channel` is the logical LightPlayer radio channel id, default `1`.
- `repeat_count` defaults to `3`, with runtime clamping to a small bounded maximum such as `8`.
- `wifi_channel` is optional and maps to `RadioConfig::new(Some(channel))`; absent means driver
  default. If an `OptionSlot<ValueSlot<u32>>` is awkward, use an optional value shape already common
  in the model crate.
- Keep the produced and consumed slots on runtime state, matching the button node's state-root
  pattern.

Register `ControlRadio` in the canonical node set:

- `NodeKind::ControlRadio`
- `NodeDef::ControlRadio(ControlRadioDef)`
- model re-exports and generated slot shape/view registration
- authored TOML kind string `ControlRadio`
- project-loader display/type name `control_radio`

## Wire Payload

Keep the existing radio packet header. Add a graph-event kind:

```rust
RadioMessageKind::ControlMessage
```

Encode the payload as fixed little-endian bytes:

```text
u32 id
u32 seq
```

Add small helpers near the radio node or in `lpc-shared`:

```rust
fn encode_control_message(message: ControlMessage, out: &mut [u8; 8]) -> &[u8]
fn decode_control_message(payload: &[u8]) -> Option<ControlMessage>
```

Prefer placing codec helpers in `lpc-shared` only if they do not introduce an unwanted dependency on
`lpc-model`. Since `lpc-shared` should stay model-agnostic, it is likely better for `ControlRadioNode` to
own the `ControlMessage` payload codec.

## Runtime Semantics

`ControlRadioNode` stores:

- current opened radio endpoint/config;
- `Box<dyn RadioDevice>`;
- pending retry entries for local messages;
- recent sent `(id, seq)` ring;
- recent received `(id, seq)` ring;
- reusable `Vec<RadioMessage>` drain buffer;
- `ControlRadioState`.

Each tick:

1. Read authored config through `ControlRadioDefView`.
2. Open or reopen the radio endpoint if endpoint/wifi channel changed.
3. Subscribe to the configured logical `RadioChannelId`.
4. Set `state.output` to an empty map and call `ctx.publish_runtime_slot(&state, output_path())`.
5. Resolve the consumed `input` map from `bus#trigger`.
6. For each local message not already pending/recently sent:
   - insert it into this tick's `output` map;
   - enqueue it with `remaining = repeat_count`.
7. Send one copy of each pending message on the configured channel, decrementing `remaining`.
8. Drain the configured channel into a reusable buffer.
9. For each `RadioMessageKind::ControlMessage`:
   - decode payload;
   - drop if in recent sent ring;
   - drop if already in recent received ring;
   - otherwise insert it into this tick's `output` map keyed by `id`.
10. Replace `ControlRadioState::output` with only this tick's accepted local and remote messages.
11. Publish `state.output` again so downstream same-frame consumers see accepted events.

The early empty publish is required for the symmetric self-edge. Without it, resolving `input` from
`bus#trigger` can expand the `radio.output -> bus#trigger` provider and re-enter the currently
executing `ControlRadioNode`.

The node should not treat radio send failures as permanent graph failure unless the device cannot be
opened or the config is invalid. A transient send failure can produce a `NodeError` for now if that
matches existing node behavior; do not silently ignore it without tests.

## Engine Services

Add `RadioService`, mirroring `ButtonService`:

```rust
pub trait RadioService {
    fn open_radio_by_spec(
        &self,
        spec: &HardwareEndpointSpec,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError>;
}
```

Implement it for `HardwareSystem`.

Add service plumbing:

- `EngineServices::set_radio_service`
- `EngineServices::radio_service`
- `TickContext::radio_service`
- pass the service through `Engine` tick paths
- set the service in `lpa-server` project new/reload paths

If `HardwareSystem` does not yet have `open_radio_by_spec`, add it using the same endpoint-spec
matching helper style as `open_button_by_spec` and `open_ws281x_by_spec`.

## Loader And Bindings

Update `ProjectLoader` to attach `ControlRadioNode` for `NodeDef::ControlRadio`.

Register binding targets:

- `input`
- `output`

Example authoring should use:

```toml
kind = "ControlRadio"
endpoint = "radio:espnow:0"
channel = 1
repeat_count = 3

[bindings.input]
source = "bus#trigger"

[bindings.output]
target = "bus#trigger"
```

Host/virtual tests may override endpoint:

```toml
endpoint = "radio:virtual:0"
```

## Main Interactions

- `ButtonNode` produces `down` into `bus#trigger`.
- `ControlRadioNode` consumes `bus#trigger`, publishes accepted local events back to `bus#trigger`, and
  sends repeated radio messages.
- Peer `ControlRadioNode` drains radio messages and produces remote accepted events into `bus#trigger`.
- `PlaylistNode` consumes `bus#trigger` and restarts the active sequence.
- If both devices run the same project, the graph remains symmetric and event-driven. No peer owns
  playlist state.

## Validation Strategy

Use virtual radio for deterministic host tests:

- local input map causes exactly `repeat_count` sent `RadioMessageKind::ControlMessage` messages;
- repeated sends carry the same `(id, seq)` payload but distinct physical radio event ids from the
  driver;
- injected duplicate remote payloads publish only once;
- accepted local and remote messages are one-tick outputs;
- local self-echo payloads are suppressed;
- endpoint/config changes reopen and resubscribe.

Then run normal host and RV32 checks without `cargo build --workspace`.

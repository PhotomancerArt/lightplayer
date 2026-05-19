# M1 Control Message And Trigger Event Design

## Scope Of Work

M1 adds the smallest slotted control-event envelope that can move through the model and bus. It does
not add button, playlist, radio, OSC network, or payload args yet.

## File Structure

```text
lp-core/lpc-model/src/
  control/
    mod.rs                     # new control-domain model exports
    control_message.rs         # ControlMessage + TriggerEvent alias/type
  lib.rs                       # re-export ControlMessage / TriggerEvent

lp-core/lpc-engine/src/
  nodes/shader/
    shader_input_materialize.rs # shared value/map SlotData -> shader ABI input values
    shader_node.rs              # visual uses shared shader input materialization
    compute_shader_node.rs      # compute uses shared shader input materialization
  dataflow/
    resolver/...               # only if message bus routing needs a small fix

examples/events/
  project.toml
  clock.toml
  event_a.toml
  event_a.glsl
  event_b.toml
  event_b.glsl
  shader.toml
  shader.glsl
  fixture.toml
  output.toml

docs/roadmaps/2026-05-19-events-playlists-radio-nodes/
  m1-control-message-trigger-event/
    00-notes.md
    00-design.md
    01-control-message-slot-shape.md
    02-bus-message-routing-tests.md
    03-cleanup-validation.md
```

## Architecture Summary

M1 defines one minimal graph-level control message envelope:

```rust
pub struct ControlMessage {
    id: u32,
    seq: u32,
    // address: OscAddress,   // deferred
    // args: Vec<ControlArg>, // deferred
    // source: String,        // deferred / host-side only
}

pub type TriggerEvent = ControlMessage;
```

This is slotted, bus-safe, and GLSL-compatible. The bus channel supplies first-slice meaning:

```text
bus#trigger carries TriggerEvent/ControlMessage values
```

The event itself does not yet carry `address` or `args`. That is a deliberate deadline cut that
keeps OSC compatibility open without forcing OSC implementation now.

## Main Components And Interactions

### ControlMessage

`ControlMessage` should:

- live in `lpc-model`,
- derive or manually implement serde, clone, debug, partial eq,
- expose constructors/accessors:
  - `ControlMessage::new(id, seq)`
  - `id()`
  - `seq()`
- implement `ToLpValue`, `FromLpValue`, and `SlotValue`.
- use existing `u32` / `LpValue::U32` support for `id` and `seq`.
- use `id` as the sentinel map key field, matching `FluidEmitter`.
- use `LpValue::Struct` for portable representation rather than adding a new top-level
  `LpValue::ControlMessage`.

### TriggerEvent

For M1, `TriggerEvent` should be either:

- `pub type TriggerEvent = ControlMessage`, or
- a newtype wrapper if the type alias creates poor generated slot names.

Prefer the alias first unless implementation proves it awkward.

### Bus Semantics

M1 should prove that a message can be routed through existing authored bindings:

```text
producer.output -> bus#trigger -> consumer.input
```

If the current bus/resolver path cannot carry non-shader `LpValue` values, apply the smallest fix
needed and test it. Do not redesign bus priority, persistence, multi-consumer event queues, or
pattern routing in M1.

### Shared Shader Map Consumption

M1 should add the inverse of the existing compute produced-map materialization path:

```text
SlotData::Map<u32, ControlMessage> -> LpsValueF32::Array([ControlMessage; len])
```

Use the existing sentinel mapping metadata (`len`, key field, `empty_key`) to build a fixed array
uniform for both compute and visual shaders. Empty entries should use a default value whose key field equals
`empty_key`; for `ControlMessage`, that is `id = empty_key` and `seq = 0`.

Both `ShaderNode::update_visual_uniforms` and `ComputeShaderNode::collect_inputs` should call this
shared helper. The helper should be generic over shader-compatible native shapes and must not know
about `ControlMessage` specifically.

### Example Project

Add a checked-in example that proves the feature in the same shape users will author:

```text
clock.seconds -> bus#time.seconds
event_a.events -> bus#trigger
event_b.events -> bus#trigger
bus#trigger -> shader.events
shader.output -> bus#visual.out -> fixture -> output
```

The visual shader should support eight event slots and draw a colored circle for each non-empty
slot. Use deterministic positions/colors derived from the slot index, `id`, and `seq` so the image
is useful as a quick smoke test. The visual should be intentionally simple: active event equals
visible circle, empty sentinel slot equals no circle.

## Deferred Design

- `address`: implied by bus channel in M1; future OSC-compatible address field.
- `args`: future OSC-compatible typed positional arguments.
- `source`: future semantic/display identity. GLSL will likely see either no source field or a
  compact numeric/binary projection, while host-side control messages can default it to an empty
  string if needed.
- `timestamp` / `timetag`: future timed/bundled messages.
- Message family enum: future work once there is more than trigger/no-args.
- Network OSC, MIDI, and DMX bridges.
- Wider ID/sequence fields, such as `u64`, if future bridge semantics need them.
- Dynamic event slot counts beyond the authored sentinel length.

# M3 Radio Node Notes

## Scope Of Work

Add the final first-generation fyeah-sign node type: an authored radio node that bridges local
`ControlMessage` trigger maps to the shared radio API and publishes accepted local plus remote radio
trigger maps back into the graph.

In scope:

- Add a core `ControlRadio` node definition in `lpc-model`.
- Add a runtime `ControlRadioNode` in `lpc-engine`.
- Expose radio hardware through `EngineServices` / `TickContext`, matching the existing button
  service pattern.
- Encode and decode `ControlMessage { id, seq }` over the existing fixed-size radio payload.
- Send each new local trigger a small fixed number of times.
- Deduplicate repeated remote trigger payloads before publishing them to graph consumers.
- Add a button/radio/playlist example or extend the in-progress button-playlist example once M2
  files exist.
- Validate with virtual radio driver tests and normal RV32 checks.

Out of scope:

- Acknowledgements, resend-until-ack, ownership, TTL, mesh routing, leader election, or durable
  distributed state.
- Full OSC address/args, timetags, bundles, or transport-level event schema migration.
- Host precompilation or weakening the on-device GLSL compiler path.
- Replacing the existing radio packet header or ESP-NOW driver.

## Current Codebase State

- M1 added `ControlMessage { id: u32, seq: u32 }` and `TriggerEvent` in
  `lp-core/lpc-model/src/control/control_message.rs`.
- M1 proved `MapSlot<u32, ControlMessage>` can travel over `bus#trigger` and into shader ABI
  sentinel maps.
- The button node exists:
  - model: `lp-core/lpc-model/src/nodes/button/button_def.rs`
  - runtime: `lp-core/lpc-engine/src/nodes/button/button_node.rs`
  - outputs: `down`, `held`, and `up`, all produced `MapSlot<u32, ControlMessage>`.
- M2 playlist work is underway. Its notes say playlist should consume the button `down` edge for
  restart semantics.
- Radio hardware already exists below the node layer:
  - shared API and fixed packet codec in `lp-core/lpc-shared/src/hardware/radio_*`
  - virtual driver in `lp-core/lpc-shared/src/hardware/virtual_radio_driver.rs`
  - ESP-NOW driver in `lp-fw/fw-esp32/src/hardware/espnow_radio_driver.rs`
- `RadioMessage` currently has a fixed header containing source device, radio event id, logical
  channel id, kind, and a bounded 64-byte payload.
- ESP-NOW receive already deduplicates identical physical radio packets by
  `(source_mac, source_device_id, radio_event_id)`.
- That driver-level dedupe is not enough for intentional repeated sends because retries will have
  distinct radio event ids. The node must dedupe by graph event identity, meaning the decoded
  `ControlMessage { id, seq }`.
- `EngineServices` currently exposes output, time, and button services. It does not yet expose radio
  services to runtime nodes.

## User Notes

- Button node is done.
- Playlist node is underway.
- Radio is the final node type needed for the fyeah sign.
- This should be a bidirectional event sync node.
- The design intent is that button and sign run the same code.
- No device owns the data. The behavior is strictly event-driven.
- The playlist's return-to-idle behavior makes the system self-healing, so long-term sync drift is
  not a first-slice problem.
- We should think about deduping and reliability, but defer deeper radio sync, ownership, TTL,
  retransmit, and mesh semantics.
- The immediate goal is to get an event out to whoever is listening.
- It is acceptable to choose repeated fire-and-dedupe instead of ack/resend for this slice.

## Decisions

### D1. Use One Bidirectional Authored Node

Use a single authored `ControlRadio` node with one consumed trigger-map input and one produced combined
event-map output.

Reason: the fyeah sign wants the button and sign to run the same project/code. Separate send and
receive nodes are mechanically clean, but they force more authoring shape and make the symmetric
project harder to read. The runtime can still keep send and receive helpers separate internally.

### D2. Use One Shared `bus#trigger`

The fyeah example should use one symmetric trigger bus:

```text
button.down      -> bus#trigger
radio.input      <- bus#trigger
radio.output     -> bus#trigger
playlist.trigger <- bus#trigger
```

This can work with the current resolver, but only if `ControlRadioNode` handles its self-edge deliberately:

1. At the start of tick, set `state.output` empty and `publish_runtime_slot("output")`.
2. Resolve consumed `input` from `bus#trigger`.
3. Accept new local messages for retransmit, drain remote messages, and write accepted events into
   `state.output`.
4. Publish `state.output` again so downstream consumers see the same-frame accepted events.

Reason: resolving `radio.input <- bus#trigger` expands bus providers. Since `radio.output` is also a
provider for `bus#trigger`, the resolver may ask for `radio.output` while the radio node is already
executing. The early empty publish satisfies that same-frame produced-slot query from cache and
prevents re-entry. The later publish replaces the produced-slot cache for playlist/shader consumers.

### D3. Use Repeated Broadcasts, Not Ack

For each newly observed local `(id, seq)`, send the same encoded `ControlMessage` for
`repeat_count` ticks. Default `repeat_count = 3`.

Reason: this is tiny, no-alloc friendly, and good enough for "thing happened" ESP-NOW packets. Ack
and retransmit need device identity, pending windows, timeouts, and policy that belong in the next
radio-sync pass.

### D4. Dedupe At The Graph Event Layer

The receiver should remember a bounded ring of recently published `ControlMessage` identities and
only publish the first unseen `(id, seq)` for the configured logical channel.

Reason: radio-driver dedupe filters duplicate physical packets, but intentional repeats use new
radio event ids. The graph-level event identity is the stable pair `(id, seq)`.

### D5. Do Not Rebroadcast Remote Events

The first node should only send messages from its consumed `input` slot. It should not send messages
that arrived from the radio or messages from its produced `output` slot.

Reason: no TTL or ownership exists yet. Rebroadcasting would turn a small reliability feature into
an uncontrolled flood when multiple devices run the same project.

### D6. Suppress Self-Echoes If They Appear

Keep a bounded ring of recently sent `(id, seq)` values and drop received radio payloads matching
that ring.

Reason: the radio node already publishes local events through `output` when it accepts them for
send. If the radio stack ever loops a broadcast back to the sender, publishing it again would be a
harmless but noisy duplicate.

## Open Questions

### Q1. Should the wire kind be `ButtonPress` or a new kind?

Suggested answer: add an explicit `RadioMessageKind::ControlMessage` variant while preserving
`ButtonPress` for diagnostics/backward compatibility.

Context: M1 intentionally made the graph event generic. Reusing `ButtonPress` would leak hardware
vocabulary back into the graph layer.

### Q2. What should the authored default endpoint be?

Suggested answer: define `DEFAULT_RADIO_ENDPOINT_SPEC` as `radio:espnow:0`, matching the product
target, and let host/emu tests or examples override it with `radio:virtual:0` when needed.

Context: the button default is product-shaped (`button:gpio:D9`) rather than virtual. ESP32 already
uses `radio:espnow:0`; the virtual driver uses `radio:virtual:0`.

### Q3. Should accepted messages be published for one tick only?

Suggested answer: yes. `ControlRadioState::output` should be replaced each tick with only newly accepted
local sends plus newly accepted remote messages from that drain.

Context: playlist restart semantics want one-shot edges, not a sticky remote state value. Button
`down` already follows this shape.

### Q4. Should `ControlRadioNode` use `MapSlot<u32, ControlMessage>` keyed by message id?

Suggested answer: yes. Keep the same sentinel map convention as M1 and the button node.

Context: this keeps shader materialization and playlist consumption on the same shape, and lets
multiple independent controls coexist in one tick.

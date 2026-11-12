# Phase 2: Bus Message Routing Tests

## Scope Of Phase

In scope:

- Prove `ControlMessage` / `TriggerEvent` values can move through the existing bus/binding
  machinery.
- Use real authored compute shader nodes as event producers.
- Add consumed map support for compute and visual shader inputs using the same shared helper and
  sentinel mapping vocabulary.
- Add a checked-in example project that demonstrates compute events driving a visual shader.
- Apply the smallest bus/resolver fix needed if current code assumes shader-compatible values.

Out of scope:

- Event queue semantics.
- Multi-consumer fanout beyond existing bus behavior.
- Playlist/button/radio nodes.
- Address or args dispatch.
- Source identity.

## Code Organization Reminders

- Keep bus/resolver changes local.
- Do not redesign the bus.
- Add tests near the behavior they exercise.
- Do not add bespoke Rust runtime nodes just to fake event producers; this milestone should exercise
  the real compute-node path.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_input_materialize.rs` or a similarly scoped helper
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs`
- `lp-core/lpc-engine/src/gfx/compute_desc.rs`
- `lp-core/lpc-model/src/nodes/shader/shader_header_gen.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/resolver.rs`
- `lp-core/lpc-engine/tests/runtime_spine.rs`
- `examples/events/...`

Expected test shape:

- Add a native shader-visible `ControlMessage` value shape.
- Build two compute shader definitions that each produce a `MapSlot<u32, ControlMessage>`.
- Each compute shader writes one event into its produced map, keyed by `id`.
- Bind both produced event maps to `bus#trigger`.
- Resolve a consumed `MapSlot<u32, ControlMessage>` from `bus#trigger` with `SlotMerge::ByKey`.
- Assert both messages are present, with stable keys matching their `id` fields.
- Repeat with the same `id` and different `seq`; assert the newer message replaces the
  older value for that key when both maps include the same key.
- Add a visual shader with a consumed `MapSlot<u32, ControlMessage>` using the same sentinel mapping.
- Render or sample a tiny output that encodes the consumed `id`/`seq`, proving the visual shader can
  read the fixed event array uniform.
- Add a compute consumed-map test using the same helper, even if the example only needs compute
  producers and a visual consumer. This prevents the two shader paths from drifting.
- Add `examples/events` with two compute producers, one visual consumer, a basic fixture,
  and an output.
- The example visual shader should support eight event slots and draw one colored circle per
  non-empty slot. The circles should be simple and deterministic, with color/position/intensity
  varied by slot index and/or message fields.
- Add or extend an example/project-loader test so the example is loaded and at least one rendered or
  sampled pixel proves an active message affects the visual output.

If the existing bus path only supports `LpsValueF32`, do not force `ControlMessage` through that
legacy runtime bus. Prefer the resolver/binding path that already carries `LpValue`, or add a narrow
parallel value-bearing bus cache entry.

Map consumption should be limited to shader-ABI-compatible native value shapes. M1 does not need
strings, dynamic map length, arbitrary key domains, or pattern dispatch.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine control_message
cargo test -p lpc-engine runtime_spine
cargo test -p lpc-engine engine_services
cargo test -p lp-cli --test examples_valid
```

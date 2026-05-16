# Phase 2: Tighten The Project-Read Writer Boundary

## Scope Of Phase

In scope:

- remove remaining `SlotData` snapshot construction from the streaming detailed
  node slot writer path, if any is still present
- make helper names/comments explicit that node slot `data` is SlotCodec JSON
- ensure shape ids and slot payloads are written without serde-owned model data
- preserve manual streaming writers for large runtime/resource payloads
- if straightforward, remove the temporary per-root `Vec<u8>` by adapting the
  outer JSON writer to the model-side `SlotWrite` interface

Out of scope:

- replacing serde for `ReadLevel`, tree deltas, probes, resources, or shape
  registry snapshots
- changing `Engine::read_project`
- changing client API return types
- genericizing large binary/resource payload serialization through SlotCodec

## Code Organization Reminders

- Keep writer helpers near `project_read_stream.rs` unless reused by
  `lpc-wire`.
- If a helper becomes reusable, prefer a clearly named file in `lpc-wire/src/slot`
  or `lpc-wire/src/messages/project_read`.
- Avoid creating a generic JSON value tree in production code.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-wire/src/messages/project_read/stream_response.rs`
- `lp-core/lpc-wire/src/slot/sync.rs`

Audit the detailed node writer:

- `write_project_node_read_result_json`
- `write_slot_data_json_value`

The desired invariant is:

```text
detailed node slot root data = SlotShapeRegistry::write_slot_json_value(...)
```

If the writer allocates a temporary `Vec<u8>` for the slot JSON because
`lpc_wire::JsonValue` cannot directly implement `SlotWrite`, that is acceptable
for this phase. Avoid introducing `SlotData` as an intermediate.

If direct writer adaptation is attempted, keep the boundary narrow:

- structured slot data can stream through SlotCodec
- large resource payload bytes should continue using their manual base64
  streaming writer
- do not build a JSON tree or `SlotData` tree to connect the two writer APIs

Add comments or helper names only where they clarify this boundary.

## Validate

```bash
cargo test -p lpc-engine project_read_stream
cargo test -p lpc-wire project_read
```

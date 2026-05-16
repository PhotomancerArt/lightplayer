# Phase 1: Prove Project-Read Slot Payloads Use SlotCodec

## Scope Of Phase

In scope:

- update `lp-core/lpc-engine/src/engine/project_read_stream.rs` tests so they
  verify detailed node slot `data` payloads by reading them through
  `SlotShapeRegistry::read_slot_json`
- keep full response serde equality tests only for responses without detailed
  slot payloads, or narrow them so they do not validate slot payloads through
  `SlotData`
- add test helpers as needed to locate `nodes.slots.roots` entries in the JSON
  response

Out of scope:

- changing public wire structs
- removing serde derives
- changing transport traits

## Code Organization Reminders

- Prefer granular test helpers at the bottom of the test module.
- Keep production writer changes minimal.
- Do not add a new general parser unless a real production caller needs it.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant file:

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`

Current tests deserialize the whole streamed response:

```rust
let decoded: ProjectReadResponse =
    lpc_wire::json::from_slice(&streamed).expect("decode streamed project read");
assert_eq!(decoded, full);
```

For detailed node reads, replace or supplement that with a helper that:

1. calls `engine.write_project_read_json(request, Vec::new())`
2. parses the response as `serde_json::Value` in the test
3. finds the `nodes` result object
4. finds `slots.roots`
5. for each root:
   - read `shape` as a raw `u32` and construct `SlotShapeId::new(raw)`
   - take `data` as a JSON string/slice
   - call `engine.slot_shapes().read_slot_json(shape_id, data_json)`
   - assert the returned object shape id matches

If downcasting is useful, downcast the node definition roots to concrete model
types for at least one root. Otherwise, inspect through `SlotAccess`.

Keep a resource-payload test for streaming base64 behavior, but do not let it be
the only test covering detailed slots.

## Validate

```bash
cargo test -p lpc-engine project_read_stream
cargo test -p lpc-model slot_codec
```

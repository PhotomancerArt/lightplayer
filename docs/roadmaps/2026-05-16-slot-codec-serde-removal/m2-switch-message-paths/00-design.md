# M2 Switch Message Paths Design

## Scope

M2 switches project-read node slot payload handling to SlotCodec. The public
response envelope can remain serde-shaped during this milestone, but the slot
root `data` payloads in detailed node reads should be written and verified
through the slot registry.

## File Structure

```text
lp-core/
  lpc-engine/src/engine/
    project_read_stream.rs      # direct project-read JSON writer
    project_read_nodes.rs       # existing allocated/serde snapshot path

  lpc-wire/src/messages/project_read/
    stream_response.rs          # streaming response helpers
    node_read.rs                # typed serde response structs remain for now

  lpc-wire/src/slot/
    sync.rs                     # typed snapshot structs remain for now

  lpc-model/src/slot_codec/
    dynamic_slot_reader.rs      # registry-backed read path
    dynamic_slot_writer.rs      # registry-backed write path
```

## Architecture Summary

The server-side detailed node read already has the shape we want:

```text
SlotAccess root
    |
    v
SlotShapeRegistry::write_slot_json_value(shape_id, data, writer)
    |
    v
project-read JSON slot root { name, shape, data }
```

M2 should make that path the behavior being tested and trusted. Tests should no
longer prove slot payload compatibility by deserializing `data` into
`SlotData`. Instead they should:

1. write project-read JSON
2. locate the `nodes.slots.roots[*]` payloads in JSON
3. read the root shape id
4. feed `data` through `SlotShapeRegistry::read_slot_json`
5. downcast where useful or inspect the resulting `SlotAccess`

This preserves the current envelope while proving that model slot payloads are
not dependent on Serde.

The embedded memory constraint remains central. Structured slot data should use
the generic SlotCodec writer, but large binary/resource payloads should stay on
their existing manual streaming writers. SlotCodec is not a reason to buffer a
resource payload as JSON, `SlotData`, or `LpValue` before writing it.

## Main Components

### Direct Project Read Writer

`Engine::write_project_read_json` remains the production writer for the first
slot-codec message path. Its node-slot branch should continue writing `data`
through `SlotShapeRegistry::write_slot_json_value`.

Any helper introduced for slot root writing should live close to the current
writer unless it becomes clearly reusable from `lpc-wire`.

The current implementation uses a temporary `Vec<u8>` per slot root so the
SlotCodec writer can produce a complete JSON value before the outer project-read
writer injects it with `raw_json`. M2 may keep that bridge, but the preferred
direction is a direct adapter from the project-read JSON writer to the
model-side `SlotWrite` API so structured slot payloads can stream into the
envelope without the extra per-root buffer.

### Slot Root JSON Test Reader

Tests need a small helper to parse project-read JSON slot roots and rehydrate
them through the registry. This can start as test support in
`project_read_stream.rs`.

The helper should not become a second production parser unless a real consumer
needs it. For this milestone, test-only is enough to prove the written message
payload is SlotCodec-readable.

### Optional Transport Follow-Up

If small, add a transport hook so desktop fallback can send already-written
project-read JSON without deserializing it into `ProjectReadResponse`.

If it is not small, do not broaden M2. Record it as follow-up work for later in
the roadmap.

## Expected Behavior

- Detailed node slot roots in project-read JSON are emitted directly from
  `SlotAccess`.
- Those root payloads can be read back through `SlotShapeRegistry`.
- Tests do not rely on `SlotData` serde to validate the detailed slot payload.
- Non-slot fields may continue using serde bridges during M2.
- Runtime/resource payloads remain on manual streaming writers.

## Non-Goals

- Do not remove `SlotData`.
- Do not remove serde derives.
- Do not rewrite the whole `ProjectReadResponse` type.
- Do not switch authored TOML loading.

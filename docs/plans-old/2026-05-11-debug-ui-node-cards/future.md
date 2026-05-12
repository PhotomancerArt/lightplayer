# Future Work

## Binding-Aware Slot Badges

- **Idea:** Expose node-owned bindings in project read and render source/target badges directly on slot rows.
- **Why not now:** Bindings are not mirrored into `lpc-view` yet.
- **Useful context:** `lpc-engine/src/node/node_tree.rs`, `lpc-engine/src/dataflow/binding`, `lpc-view/src/tree`.

## Product And Resource Detail Fetches

- **Idea:** Let UI request product probes or resource payloads from a skeleton row.
- **Why not now:** This needs request state, possibly probes, and care around bandwidth.
- **Useful context:** `ProjectReadRequest::probes`, `ResourcePayloadRead`.

## Editable Slot Rows

- **Idea:** Turn writable def/config rows into mutation controls.
- **Why not now:** Mutation message flow exists in mockup concepts but is not wired into the real engine/UI yet.
- **Useful context:** `lpc-wire/src/slot/mutation.rs`, `lpc-view/src/slot/mirror.rs`.


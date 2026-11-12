# Future Work

## Raw Project-Read Frames For Local Transports

- **Idea:** Add an optional raw server-message JSON frame path for local/desktop
  transports so they do not have to deserialize streamed project-read JSON back
  into typed messages before sending.
- **Why not now:** Once slot root data has a single canonical snapshot syntax,
  the typed local transport can remain correct. Raw-frame transport cleanup is a
  separate performance/architecture concern.
- **Useful context:** `lp-core/lpc-shared/src/transport/server.rs`,
  `lp-app/lpa-client/src/local.rs`, `lp-fw/fw-esp32/src/transport.rs`.

## SlotCodec With Explicit Sync Metadata

- **Idea:** Explore an extended SlotCodec mode that carries revisions and full
  snapshot presence metadata while retaining the concise field-name syntax.
- **Why not now:** The immediate bug is caused by using the current authored
  SlotCodec syntax as a sync snapshot. A separate snapshot codec is the safer
  cleanup.
- **Useful context:** `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`,
  `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`.

# lpc-wire

Engine-client protocol model for LightPlayer core.

This crate owns the request/response and sync contract exchanged between
`lpc-engine`/`lpa-server`, firmware transports, clients, and `lpc-view`:
messages, project reads, tree deltas, slot sync payloads, and transport
errors. Outbound serialization goes through `serde` (host) or `ser-write-json`
(the ESP32 streaming writer); the crate carries no bespoke JSON writer of its
own.

It should not own domain modeling or generic slot serialization. Slot shapes,
slot access, `SlotCodec`, authored TOML, and generic JSON/TOML slot readers
live in `lpc-model`. `lpc-wire` may carry slot-shaped payloads on the protocol
surface, but it should not become a second slot/model crate.

**Naming:** Envelope and directional types (`Message`, `ClientMessage`,
`ClientRequest`, `ServerMessage`, `FsRequest`, …) already imply the wire
contract. Use `Wire*` when a noun also exists in model/source/view/engine form
and needs disambiguation — for example `WireTreeDelta`,
`LegacyWireNodeSpecifier`, `WireSlotIndex`.

`no_std`, designed for embedded-compatible transports. It should not depend on
`lps-shared`; runtime values must cross the `lpc-engine` boundary through
`lpc-model` shapes such as `LpValue`, `LpType`, and slot snapshots.

**The fs wire** (`server/fs_api.rs`): reads, writes and deletes are stateless
requests, with one exception since wire 42 — a push's **batch**
(`beginBatch`/`commitBatch`/`abortBatch`, answered `Batch { op, atomic,
error }`), which the server holds for the link that began it. Chunked writes
come plain (`writeChunk`) or as raw deflate at logical offsets
(`writeChunkDeflated`, answered like a `writeChunk`); `budget.rs` holds the
chunk sizes, the deflated chunk's logical cap and the record hint a push
plans for. See `docs/adr/2026-10-10-fs-push-boundary-and-deflated-writes.md`.

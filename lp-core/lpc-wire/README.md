# lpc-wire

Engine-client protocol model for LightPlayer core.

This crate owns the request/response and sync contract exchanged between
`lpc-engine`/`lpa-server`, firmware transports, clients, and `lpc-view`:
messages, project reads, tree deltas, slot sync payloads, transport errors,
and bounded JSON writers for wire emission.

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

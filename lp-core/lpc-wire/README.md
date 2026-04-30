# lpc-wire

Engine-client wire model for LightPlayer core.

This crate owns the serializable request/response and sync shapes exchanged
between `lpc-engine`/`lpa-server` and `lpc-view`: messages, project
views, tree deltas, transport errors, JSON helpers, and legacy partial state
serialization helpers.

**Naming:** Envelope and directional types (`Message`, `ClientMessage`,
`ClientRequest`, `ServerMessage`, `FsRequest`, …) already imply the wire
contract. Use `Wire*` when a noun also exists in model/source/view/engine form
and needs disambiguation — for example `WireTreeDelta`, `WireNodeStatus`,
`WireNodeSpecifier`, `WireSlotIndex`.

`no_std`, designed for embedded-compatible transports. It should not depend on
`lps-shared`; runtime values must cross the `lpc-engine` boundary as
`lpc-model::ModelValue`.

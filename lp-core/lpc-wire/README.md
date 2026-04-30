# lpc-wire

Engine-client wire model for LightPlayer core.

This crate owns the serializable request/response and sync shapes exchanged
between `lpc-engine`/`lp-server` and `lpc-view`: messages, project
views, tree deltas, transport errors, JSON helpers, and legacy partial state
serialization helpers.

Use `Wire*` names for types whose role would otherwise be ambiguous.

`no_std`, designed for embedded-compatible transports. It should not depend on
`lps-shared`; runtime values must cross the `lpc-engine` boundary as
`lpc-model::WireValue`.

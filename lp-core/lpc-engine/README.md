# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: project runtime state, node trees,
resolver caches, buses, runtime property access, and the boundary between
shader/runtime values and portable model or wire values.

Unlike `lpc-model`, `lpc-source`, and `lpc-wire`, this crate may depend on
`lps-shared` because it is responsible for converting between `LpsValue` /
`LpsType` and `WireValue` / `WireType`.
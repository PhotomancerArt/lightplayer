# lpc-source

Authored LightPlayer source model: the persisted on-disk project and artifact
formats that the engine loads.

This crate owns source-side concepts such as artifacts, slots, source bindings,
source node config, value specs, TOML parsing, and schema migration.

Use `Src*` names for types whose role would otherwise be ambiguous.

`no_std`, designed for embedded-compatible loading and tooling. It should not
depend on `lps-shared`; source values use `lpc-model` portable value/type
shapes and are materialized by `lpc-engine`.

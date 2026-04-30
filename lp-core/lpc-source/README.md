# lpc-source

Authored LightPlayer source model: the persisted on-disk project and artifact
formats that the engine loads.

This crate owns source-side concepts such as artifacts, slots, source bindings,
source node config, value specs, TOML parsing, and schema migration.

**Naming:** Exported authored/source-specific types use the `Src*` prefix
(`SrcArtifact`, `SrcArtifactSpec`, `SrcBinding`, `SrcShape`, `SrcSlot`,
`SrcValueSpec`, …). Do not introduce short root aliases (for example
`ValueSpec = SrcValueSpec`); imports should use the canonical `Src*` names.

`no_std`, designed for embedded-compatible loading and tooling. It should not
depend on `lps-shared`; source values use `lpc-model` portable value/type
shapes and are materialized by `lpc-engine`.

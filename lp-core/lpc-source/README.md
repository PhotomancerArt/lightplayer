# lpc-source

Authored LightPlayer source model: the persisted on-disk project and artifact
formats that the engine loads.

This crate owns source-side mechanics such as artifact IO, TOML parsing,
source-only value specs, and schema migration. Durable node-definition concepts
increasingly live in `lpc-model` so source files, wire sync, and tooling share
one semantic model.

**Naming:** Keep the `Src*` prefix for genuinely source-specific compatibility
types (`SrcArtifact`, `SrcValueSpec`, old `SrcShape` / `SrcSlot`, …). Do not add
new durable domain vocabulary here when it belongs in `lpc-model`.

`no_std`, designed for embedded-compatible loading and tooling. It should not
depend on `lps-shared`; source values use `lpc-model` portable value/type
shapes and are materialized by `lpc-engine`.

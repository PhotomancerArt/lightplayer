# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: project runtime state, node trees,
resolver caches, buses, runtime property access, and the boundary between
shader/runtime values and portable model or wire values.

**Spine (M4.3):** `node` defines the new `Node` trait and tick/context
types. `artifact` holds `ArtifactManager`, `ArtifactLocation`, `ArtifactId`,
and source-load helpers. `resolver` implements the consumed-slot binding
cascade. `tree::NodeEntry` carries `SrcNodeConfig`, artifact handles, and
`ResolverCache` on the generic spine path. Legacy visual runtimes remain under
`nodes` (`LegacyNodeRuntime`) beside this spine until M5 cutover.

Unlike `lpc-model`, `lpc-source`, and `lpc-wire`, this crate may depend on
`lps-shared` because it is responsible for converting between `LpsValue` /
`LpsType` and `ModelValue` / `ModelType`.

**Naming:** Prefer plain engine/runtime nouns when the crate already owns the
concept (`ProjectRuntime`, `NodeTree`, `ResolverCache`, `Bus`). Use an `Engine*`
prefix only when ambiguity with another layer remains high. Conversion helpers
should name both sides of the boundary (for example functions that mention
`model_value` / `ModelType` vs `LpsValueF32` / `LpsType`).
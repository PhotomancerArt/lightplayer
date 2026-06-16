# ADR 2026-06-12: Effective Asset Materialization

## Status

Accepted

## Context

Assets are now first-class project inventory entries. Shader source files,
compute shader source files, fixture SVG mappings, future images, and inline
source text all use the `AssetLocation` and `AssetContentType` model vocabulary.

Current engine loading still materializes shader source and fixture SVG files by
reading paths directly from the filesystem. That duplicates registry knowledge
about overlay replacement, overlay deletion, inline assets, committed artifact
revisions, and error states.

The engine cutover should not recreate asset precedence rules for every runtime
node kind.

## Decision

`ProjectRegistry` owns effective asset materialization.

The registry should provide engine-facing APIs that materialize current
effective asset bytes and text from an `AssetLocation`, such as:

```rust
ProjectRegistry::materialize_asset(...)
ProjectRegistry::materialize_asset_text(...)
```

Those APIs should honor the same effective project state as the inventory:

- artifact-backed asset plus overlay replacement returns overlay bytes at the
  overlay revision;
- artifact-backed asset plus overlay delete returns a deleted error;
- artifact-backed asset without overlay reads bytes transiently through the
  registry `ArtifactStore` and reports the artifact revision;
- inline source assets read from the effective owner definition and report the
  owner definition revision;
- unknown or unreferenced assets return a clear error unless a later API
  intentionally permits ad hoc reads.

Source-file helpers may remain named `source` when they specifically deal with
authored source text and string diagnostics, but they sit under the broader
asset model.

The public registry materialization boundary is generic over effective assets;
source-specific helpers are allowed only where the caller truly needs UTF-8
source text or source-specific diagnostics.

## Consequences

Runtime node attachment uses one registry-owned materialization path for shader
source, compute shader source, fixture SVG mappings, and future asset kinds.

Overlay replacement and deletion affect asset consumers consistently before
commit.

Asset revisions come from the registry's effective state instead of a separate
engine artifact cache.

The future UI can reason about files, project inventory, and runtime consumers
using the same asset identities.

Authored `AssetSlot` discovery can evolve internally while preserving the
registry materialization API consumed by the engine.

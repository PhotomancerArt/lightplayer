# Stateless Project Read Sync Summary

## What Was Built

- Added the canonical stateless `ProjectReadRequest` / `ProjectReadResponse`
  wire vocabulary.
- Added node, shape, and resource read query/result domains.
- Added request-scoped probe vocabulary with `RenderProduct` and `ExplainSlot`
  probes plus commented future variants.
- Added `Engine::read_project` and helpers for shapes, nodes, resources, and
  unsupported probe responses.
- Updated `ProjectView` to be node-centric around tree, slot mirror, and
  resource cache.
- Added project read application helpers in `lpc-view`.
- Routed `WireProjectRequest::Read` through `lpa-server` and `lpa-client`.
- Removed old disabled-sync, watch-specifier, and resource-specifier
  vocabulary.
- Added `docs/lp-core/probes.md`.

## Decisions For Future Reference

#### Node-Centric Read Model

- **Decision:** The read protocol exposes nodes, shapes, and resources, not
  artifacts.
- **Why:** Nodes are the project UX; filesystem APIs remain the path for direct
  file inspection/editing.
- **Rejected alternatives:** Artifact query domain in the first protocol.
- **Revisit when:** Inline node definitions or artifact authoring tools need a
  dedicated source inspector.

#### Probes Are Request-Scoped Diagnostics

- **Decision:** Probes live beside normal read queries and are not part of the
  client project mirror.
- **Why:** Product rendering, slot explanation, shader traces, fs probes, and IO
  probes are explicit diagnostic work, not subscriptions or authored state.
- **Rejected alternatives:** Debug slots such as `state.debug_texture` as the
  canonical path.
- **Revisit when:** Streaming probe responses or persistent diagnostic sessions
  become necessary.

#### RenderProduct Probe Name

- **Decision:** The product materialization probe is named `RenderProduct`, not
  `VisualTexture`.
- **Why:** Textures are resources; the probe asks a product to render into an
  inspection format.
- **Rejected alternatives:** `VisualTextureProbe`.
- **Revisit when:** Products split into more specialized render/control
  families with different probe semantics.

#### Coarse First Read Sync

- **Decision:** The first implementation uses full snapshots/coarse results
  where fine-grained diffs are not ready.
- **Why:** The stateless API shape matters first; slot and registry diff
  optimization can follow without changing the envelope.
- **Rejected alternatives:** Implementing minimal patches for every domain in
  the first pass.
- **Revisit when:** Low-bandwidth UI polling starts moving enough data to hurt.

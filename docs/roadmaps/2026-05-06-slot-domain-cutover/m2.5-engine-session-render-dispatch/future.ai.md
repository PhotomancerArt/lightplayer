## Texture Registry

- **Idea:** Add a first-class `TextureRegistry` for materialized textures and cached texture-backed products.
- **Why not now:** This milestone removes render-product registry indirection from node render flow; concrete texture resource ownership is a separate design.
- **Useful context:** `TextureRenderProduct` remains useful as a materialized response type, but it should not define render-product identity.

## Fixture-To-Output Product

- **Idea:** Add a `ControlProduct`, `DmxProduct`, or similar product to wire fixture output into output nodes through the same dataflow model.
- **Why not now:** Current fixture-to-output buffer writing is awkward but working; this milestone is about shader render ownership and session boundaries.
- **Useful context:** The user expects fixture/output to become cleaner later and noted this likely removes current direct output-buffer weirdness.

## Async-Like Runtime

- **Idea:** Explore real async/coroutine-like node execution if synchronous reentrant session calls become too hard to reason about.
- **Why not now:** A synchronous `EngineSession` with explicit request/call boundaries gives the needed await-like behavior without an embedded executor or future lifetime complexity.
- **Useful context:** Node-level calls are not a hot path, so ergonomics matter, but same-node re-entry is intentionally out of scope.


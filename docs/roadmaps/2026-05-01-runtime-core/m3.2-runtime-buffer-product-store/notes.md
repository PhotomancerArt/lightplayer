# M3.2: Runtime Buffer Product Store

## Purpose

Define the minimal runtime-owned storage pattern for texture-like products and
large color/raw buffers before the legacy MVP nodes move onto the core engine.

M4 should not push texture pixels, fixture lamp colors, or output channel bytes
through scalar value or generic node-state paths by accident.

## Working Scope

- Decide the minimal product/buffer identity model: IDs, metadata, versions, and
  ownership.
- Extend or complement `RenderProductStore` so texture-backed compatibility can
  be represented without committing to the final render-product family.
- Define what the wire layer sees: references, snapshots, diffs, or legacy
  compatibility views.
- Avoid deep transport policy now: compression, scaling, throttling, and binary
  chunking should remain future work unless needed for the shape.

## Handoff To M3.3

M3.3 should be able to build adapter/harness nodes that publish and consume
store-backed products without inventing new storage rules.

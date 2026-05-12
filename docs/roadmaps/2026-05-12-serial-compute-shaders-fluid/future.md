## GPU-Style Parallel Compute

- **Idea:** Add dispatch grids, workgroups, storage buffers, and wgpu-backed compute semantics.
- **Why not now:** Serial compute is enough to validate the domain and avoids a much larger ABI/runtime surface.
- **Useful context:** The first compute shader should be a per-frame data program, not a GPU workgroup program.

## Shader-Source-First Shape Extraction

- **Idea:** Infer shader slots and structs from GLSL declarations or annotations.
- **Why not now:** TOML is the source of truth for shape; source-first parsing would complicate UI editing and versioning.
- **Useful context:** UI-generated header regions provide most of the ergonomics without moving ownership into GLSL.

## ComputeProduct

- **Idea:** Introduce a lazy `ComputeProduct` handle for expensive or large compute outputs.
- **Why not now:** First serial compute outputs can be published as produced slot values directly.
- **Useful context:** Add this when compute outputs become expensive, lazily requested, or too large to copy per frame.

## Native Emitter Nodes

- **Idea:** Add purpose-built Rust emitter nodes for tests, low-cost embedded effects, or non-shader use cases.
- **Why not now:** The roadmap intentionally pressures compute shaders as the source of emitter data.
- **Useful context:** Native emitter nodes remain useful fallback and test fixtures.

## Touch And Audio Inputs

- **Idea:** Drive fluid emitters from touch, audio analysis, MIDI/OSC, or other input nodes.
- **Why not now:** Requires input-node semantics and likely richer bus/concurrency behavior.
- **Useful context:** Fluid emitters should be shaped so these future sources can produce the same value.


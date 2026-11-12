# M3 Compute Shader Node Summary

## What Was Built

- Added an engine-side `LpComputeShader` trait and `LpGraphics::compile_compute_shader`.
- Implemented compute compilation for host wasm, wasm guest, and native RV32 graphics backends.
- Added `ComputeShaderNode` for `kind = "shader/compute"` artifacts.
- Added dynamic compute runtime state roots shaped by authored produced slots.
- Materialized produced value slots and sentinel-array map slots into `SlotData`.
- Wired project loading for compute shader artifacts, including generated shader headers.
- Exported `ComputeShaderDefView` alongside the other node definition views.
- Added tests for loader integration, compute execution, and sentinel array to slot map materialization.

## Decisions For Future Reference

#### Maps Stay Above The Shader ABI

- **Decision:** `lp-shader` returns raw `LpsValueF32`; the engine materializes semantic slot maps.
- **Why:** Maps, merge semantics, and slot revisions are LightPlayer dataflow concepts, not shader ABI concepts.
- **Rejected alternatives:** Teaching `lp-shader` about `SlotMapDyn` or `FluidEmitter` collections.
- **Revisit when:** We design richer shader ABI projections or non-sentinel map mappings.

#### Compute State Is Dynamic

- **Decision:** Each `ComputeShaderNode` registers an instance-specific runtime state shape.
- **Why:** Produced slots are authored by each compute shader artifact, so a static Rust state shape would be false.
- **Rejected alternatives:** A fixed `ComputeShaderState` Rust struct with known fields.
- **Revisit when:** Artifact reload needs stable dynamic shape ids across replacements.

#### Produced Map Resolution Is Still Slot-Data Only

- **Decision:** M3 exposes produced maps in node runtime state snapshots but does not make `Production` carry non-leaf slot data.
- **Why:** The current resolver returns versioned `LpValue` leaves. Non-leaf bindings and merge strategies need their own focused pass.
- **Rejected alternatives:** Quickly widening `Production` to carry arbitrary `SlotData`.
- **Revisit when:** Fluid nodes consume compute-produced emitter maps through normal bindings.

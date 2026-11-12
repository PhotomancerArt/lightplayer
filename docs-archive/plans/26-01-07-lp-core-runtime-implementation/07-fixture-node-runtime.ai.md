# Phase 7: Fixture Node Runtime

## Goal

Implement FixtureNodeRuntime and FixtureNodeConfig, including SamplingKernel for texture sampling.

## Tasks

1. Create `nodes/fixture/config.rs`:
   - Move `FixtureNode` enum and `Mapping` struct from `nodes/fixture.rs` to here
   - Rename to `FixtureNodeConfig` (or keep as `FixtureNode` if that's the pattern)
   - Ensure `output_id` uses `OutputId` and `texture_id` uses `TextureId`
   - Export from `nodes/fixture/mod.rs`

2. Create `nodes/fixture/runtime.rs` with:
   - `SamplingKernel` struct:
     - `radius: f32` (normalized sampling radius, same for all pixels)
     - `samples: Vec<SamplePoint>` (precomputed sample points)
   - `SamplePoint` struct:
     - `offset_u: f32`, `offset_v: f32` (relative offsets)
     - `weight: f32`
   - `FixtureNodeRuntime` struct:
     - `output_id: OutputId`
     - `texture_id: TextureId`
     - `kernel: SamplingKernel`
     - `channel_order: String` (e.g., "rgb")
     - `mapping: Vec<Mapping>` (from config)
     - `status: NodeStatus`
   - Implement `NodeLifecycle` trait:
     - `Config = FixtureNodeConfig`
     - `RenderContext = FixtureRenderContext`
     - `init()`:
       - Precomputes `SamplingKernel` from config radius (one kernel reused for all mapping points)
       - Stores mapping from config
     - `update()`:
       - Gets texture via `ctx.get_texture(texture_id)` (read-only)
       - Gets output via `ctx.get_output_mut(output_id)`
       - For each mapping point:
         - Samples texture at `center + kernel.samples` positions
         - Averages samples (weighted by kernel weights)
         - Writes to output buffer based on `channel_order` and `mapping.channel`
       - Handles errors (set status to Error)
     - `destroy()`: Cleanup if needed

3. Update `nodes/fixture/mod.rs` to export config and runtime

4. Add tests:
   - Test SamplingKernel precomputation
   - Test FixtureNodeRuntime::init() creates kernel correctly
   - Test update() samples texture and writes to output buffer
   - Test channel_order handling (rgb, rgba, etc.)
   - Test error handling (missing texture, missing output)

## Success Criteria

- FixtureNodeRuntime compiles and implements NodeLifecycle
- SamplingKernel precomputation works correctly
- Texture sampling works correctly
- Output buffer writing works correctly
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style


# Implementation Questions

## Question 1: Implementation Order ✅ ANSWERED

**Answer**: Break into phases.

## Proposed Phase Breakdown

1. **Type-safe IDs and FrameTime** - Implement `nodes/id.rs` with all ID types and `runtime/frame_time.rs` (+ tests)
2. **Texture Utility** - Implement `util/texture.rs` with Texture struct and methods (+ tests)
3. **Lifecycle Trait and Contexts** - Implement `runtime/lifecycle.rs` and `runtime/contexts.rs` (InitContext and render contexts) (+ tests)
4. **Texture Node Runtime** - Implement `nodes/texture/runtime.rs` and `nodes/texture/config.rs` (+ tests)
5. **Output Node Runtime** - Implement `nodes/output/runtime.rs` and `nodes/output/config.rs` (including OutputProvider trait) (+ tests)
6. **Shader Node Runtime** - Implement `nodes/shader/runtime.rs` and `nodes/shader/config.rs` (uses `GlslExecutable` from `lp-glsl-compiler` crate) (+ tests)
7. **Fixture Node Runtime** - Implement `nodes/fixture/runtime.rs` and `nodes/fixture/config.rs` (including SamplingKernel) (+ tests)
8. **ProjectRuntime** - Implement `project/runtime.rs` with init, update, destroy methods (+ tests)
9. **ProjectBuilder** - Implement `builder.rs` with fluent API (+ tests)
10. **Integration and Cleanup** - Link everything together, integration tests, cleanup

**Note**: `GlslExecutable` trait already exists in `lp-glsl-compiler` crate, so we can use that directly.

**Question**: Does this phase breakdown look good? Any changes needed?


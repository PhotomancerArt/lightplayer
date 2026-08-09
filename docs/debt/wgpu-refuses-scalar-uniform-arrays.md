# wgpu tier refuses bare scalar/vec2 uniform arrays

**Condition.** On the GPU tier, `layout(binding = N) uniform float x[K];`
(and any element whose natural stride is not 16-aligned: int/uint/bool,
vec2/ivec2, scalar-only structs) fails naga validation before any LightPlayer
code runs: the GLSL frontend gives bare (non-block) uniform arrays their
natural stride (`third_party/naga/src/front/glsl/parser/types.rs` →
`Layouter::to_stride`, 4 for float), while the uniform address space requires
16 (`valid/type.rs` `Disalignment::ArrayStride`). The std140 stride rounding
in `front/glsl/offset.rs` only runs for interface-block members, which
top-level uniforms never are.

vec3/vec4/mat and 16-aligned-struct arrays pass — `examples/meteor`'s
`uniform Meteor meteors[4]` works only because the struct is 16-aligned.

**Why it matters now.** Buffer slots (ADR 2026-08-08-typed-shader-buffers)
make scalar arrays the natural shape for per-cell state consumed by visual
shaders. On the CPU tiers (native lpvm, fw-browser wasm — the tiers previews
actually run) they work; on the wgpu tier the shader fails to compile with
naga's stride error, exactly as any `float[N]` uniform already did before
buffers. The gap is pre-existing; buffers only make it more likely to be hit.

**Also unexercised:** `lp-gfx-wgpu/src/uniform_writer.rs`'s array branch has
no test anywhere encoding an array uniform (the buffer branch mirrors it).

**The paid-down shape.** Fix tier-side, in the wgpu assembly/lowering layer,
so authored GLSL stays canonical: pack `float x[N]` as `vec4 x_packed[ceil(N/4)]`
plus a spliced accessor (or re-declare inside a std140-legal block form) at
WGSL assembly time. The uniform writer already reads naga's own
stride/offset numbers, so the value side follows automatically.

**Trigger to revisit.** GPU preview tier becoming a default surface, or the
first buffer-consuming visual shader that must run on wgpu.

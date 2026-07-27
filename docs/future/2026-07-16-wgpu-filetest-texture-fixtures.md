# wgpu.f32 filetest texture fixtures (deferred)

## Status: deliberately deferred (2026-07-16, M6 P3 review)

## What is deferred

34 directives across the `texture/` filetest corpus carry
`@unsupported(wgpu.f32)` with the rationale "texture fixtures are not
bound through the GPU registry yet". The `wgpu.f32` probe target
(lps-filetests `test_run/wgpu_probe.rs`) rejects any file with texture
specs instead of binding its inline pixel fixtures.

## Why

The filetest fixture-binding path is written against **guest memory**:
`alloc_shared()` places fixture pixels in the compiled shader's arena and
the sampler uniform carries a real guest pointer. The wgpu backend has no
guest memory — textures live in the backend registry
(`GpuGraphics::create_texture` → `texture_uniform_value()` →
`LpsValueF32::Texture2D`, resolved to a bind-group entry at render time).
Supporting fixtures needs a parallel binding path, not an adaptation.

Deferral was judged low-cost: GPU sampling semantics (wrap modes,
`texelFetch` edge clamp, formats, filtering over `textureLoad`) are
already pinned by the M5 gate — `lp-gfx-wgpu/tests/texture_corpus.rs` —
so the filetest run would re-verify covered behavior through a second
harness rather than add coverage. Nothing on the lp-gfx or Studio
roadmaps consumes it.

## What un-deferring takes (sm)

1. Branch the harness fixture binding on the wgpu instance: create the
   texture through the shared probe `GpuGraphics`, keep the
   `TextureHandle` alive for the instance lifetime, take
   `texture_uniform_value()`.
2. Teach `WgpuProbeInstance`'s uniform-tree builder to carry
   `Texture2D` values and dotted paths for struct-nested samplers
   (`params.gradient`) — today it builds a flat tree and rejects dotted
   `set_uniform` paths.
3. Make `EXPECT_SETUP_FAILURE` files fail at bind time with matching
   error text.
4. Remove the 34 annotations (greppable by the rationale string) and
   rerun `--target wgpu.f32`.

Natural moment: fold into the assembly prototype-order fix (which removes
~120 sibling annotations in the same motion).

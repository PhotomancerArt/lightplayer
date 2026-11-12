# Notes

## Scope and Goals

Design the texture-read surface for `lp-shader`: how shader programs read from
texture resources, how that maps to GLSL and future WGSL/wgpu, and which
features are supported first on the RV32/Q32 CPU path.

The design should be a long-lived reference for roadmaps and implementation
plans. It should describe:

- Why texture reads are needed for LightPlayer visuals.
- The intended user-facing shader surface in GLSL today and WGSL later.
- The internal model: texture resource, compile-time format, runtime dimensions,
  filter/wrap policy, and shader-visible binding.
- CPU/Q32 performance constraints and which operations are cheap vs expensive.
- A clear feature priority list for palettes, effects, and transitions.
- Explicit non-goals such as true auto-LOD mipmapping, 3D textures, cubemaps,
  and broad sampler feature parity.

## Current System State

- `lp-shader` already has a high-level `LpsEngine` wrapper that compiles GLSL
  through `lps-frontend`, lowers to LPIR, and compiles through an `LpvmEngine`.
- The write path exists: `compile_px` validates `render(vec2 pos)` and
  synthesises a format-specific `__render_texture_<format>` function that
  writes to an output texture.
- `TextureStorageFormat` currently includes `R16Unorm`, `Rgb16Unorm`, and
  `Rgba16Unorm`. Output format is compile-time-selected for efficient stores.
- `LpsTextureBuf` wraps an `LpvmBuffer`; texture memory is already
  guest-addressable and can be passed into shader code via guest pointers.
- The filetest harness currently compiles directly through LPVM backends and
  sets uniforms before calling functions. It does not yet create `LpsEngine`
  or allocate/bind texture resources.
- `lps-frontend` maps Naga types into `LpsType`, but there is no sampler/image
  or texture type in `LpsType` yet.
- Naga can parse WGSL as well as GLSL, but the current frontend path and
  filetests are GLSL-oriented.

## Constraints and Requirements

- The on-device compiler path is non-negotiable and must remain `no_std` +
  `alloc` compatible. Texture support cannot depend on host-only preprocessing
  at runtime.
- RV32/Q32 performance matters. Format dispatch, filter choice, and wrap mode
  should be compile-time or binding-time decisions where possible, not
  per-sample dynamic branches.
- Texture dimensions are runtime values. Texture storage format should be known
  at compile time for efficient load conversion.
- The design should keep eventual WGSL/wgpu support straightforward. WGSL has
  a cleaner texture/sampler model, so the internal model should not be boxed in
  by GLSL's older `sampler2D` surface.
- The v0 model should prefer 2D textures. Linear palettes and gradients can be
  represented as width-by-1 textures; 1D-specific APIs should be helper-level,
  not a separate resource kind unless proven necessary.
- Even though resources are 2D, the binding descriptor should be able to carry
  a height-one / 1D usage hint for cases like baked gradients. The runtime still
  binds a 2D texture, but the compiler can lower palette-style sampling to a
  cheaper path when the descriptor promises the resource is one pixel high.
- Filetests need a way to allocate/populate texture memory and bind texture
  descriptors while still exercising all LPVM backends. Texture filetests should
  be designed with eventual wgpu comparison in mind, so test declarations should
  describe texture resources and expected sampling behavior in backend-neutral
  terms.

## Needed Features

- 1D-optimised lerped lookup for gradients/palettes, likely as a helper or
  builtin operating on a width-by-1 2D texture. The descriptor should expose
  this as a compile-time usage/shape hint so the compiler can optimize the
  sampling path without adding a separate 1D resource type.
- 2D `texelFetch`-style integer-coordinate lookup for simple effects and
  transitions.
- 2D filtered `texture`-style normalised-coordinate sampling for effects and
  transitions that rescale, zoom, twist, or otherwise warp input textures.
- A compile-time or binding-time performance knob that can map filtered sampling
  to nearest-neighbour on CPU-constrained targets.
- Useful wrap modes for effects, especially clamp and repeat. Mirror may be a
  follow-up depending on cost and ergonomics.

## Open Questions

## Resolved Questions

- **Q1:** Internal model should be WGSL-style while GLSL remains the v0
  user-facing/filetest surface.
- **Q2:** V0 resource dimensionality should be 2D only. 1D operations are
  optimized helpers over width-by-1 textures; 3D is deferred.
- **Q3:** Mipmaps/automatic LOD are out of scope for v0. Manual LOD and
  host-baked mip chains remain future work.
- **Q4:** Palette-stop-to-texture baking belongs above `lp-shader`, likely in
  `lp-domain`/`lp-engine`, not in compiler lowering.
- **Q5:** GLSL should spell filter/wrap policy with binding metadata outside
  shader source, not custom GLSL layout qualifiers or a large family of
  LP-specific builtin names. The current `lp-domain` work already uses TOML
  visual definitions that route shader inputs (`inputColor`, `inputA`,
  `inputB`) by convention/context, and that layer can also specify or inherit
  texture format/filter/wrap policy.
- **Q6:** `lp-shader` should not model texture source routing. The compiler
  accepts a map keyed by sampler uniform name, with one descriptor per shader
  texture input. Each descriptor carries the shader-relevant facts needed for
  lowering and diagnostics: storage format, filter policy, wrap policy, and
  any other compile-time texture semantics. Higher layers such as lpfx/domain
  decide where the actual texture comes from and provide the descriptor and
  runtime binding data.
- **Q7 note:** A texture binding descriptor may include a 1D/height-one usage
  hint. This does not add a separate resource dimensionality: it tells
  `lp-shader` that a normal 2D sampler is known to be backed by a height-one
  texture, enabling cheaper gradient/palette sampling.
- **Q7:** The binding descriptor should include the shape hint, and descriptor
  promises should be enforced strictly. Compile-time descriptor/shader
  mismatches and runtime binding violations such as `HeightOne` with
  `height != 1` are hard errors. Prefer fail-fast diagnostics over permissive
  fallback behavior, at least for the initial implementation.
- **Q8:** Implementation should be foundation-first. The design document is the
  immediate product and should feed a roadmap. The first implementation slice
  should prove the descriptor contract, validation, texture binding plumbing,
  format-aware loads, and backend lowering through `texelFetch`; filtered
  `texture` sampling and palette helpers build on that foundation.
- **Q9 note:** Strong validation is part of the design, not just an
  implementation detail. Filetests are the main validation tool and should be
  extended intentionally for texture inputs, descriptor specs, runtime binding,
  and diagnostics. The filetest shape should keep a path open to validating
  behavior against wgpu later, even if the wgpu comparison runner lands after
  the CPU/RV32 implementation.
- **Q9:** Texture filetests should use inline fixtures for the small textures
  needed by compiler tests. For `rgba16unorm`, fixture data should be written
  as pixel-grouped channel values rather than raw bytes. Each pixel is
  whitespace separated; each channel inside a pixel is comma separated with no
  spaces. Prefer normalized float channels where readability matters, e.g.
  `1.0,0,0,1.0 0,1.0,0,1.0`; allow exact hex storage values where precision or
  boundary cases matter, e.g. `ffff,0000,0000,ffff 0000,ffff,0000,ffff`. The
  parser encodes those channel values into the target texture storage format.
  Sidecar fixture files can be added later if larger images need them.
- **Q10:** Core descriptor vocabulary should live in `lps-shared`, next to
  `TextureStorageFormat`: `TextureBindingSpec`, `TextureFilter`,
  `TextureWrap`, and `TextureShapeHint` (names tentative). Compilation should
  receive texture specs through a named compile descriptor such as
  `CompilePxDesc`/`LpsCompileDesc`, not by extending `compile_px` with more
  positional arguments.
- **Q11:** Runtime texture inputs are guest-visible as uniform descriptors, not
  separate hidden bind slots. A GLSL `sampler2D` maps to a logical texture
  uniform whose ABI layout contains a guest pointer plus dimensions, likely
  `{ ptr: u32, width: u32, height: u32, row_stride: u32 }`. The public
  `lp-shader` API should still expose typed helpers/values built from
  `LpsTextureBuf`; callers should not normally hand-author raw pointer structs.
  Runtime validation checks that the `LpsTextureBuf`/texture value matches the
  compile-time `TextureBindingSpec` before rendering.
- **Q12:** `Texture2D`/`sampler2D` should be a logical `LpsType`, with typed
  runtime values/helpers and diagnostics, even though its guest ABI lowers to a
  fixed uniform descriptor struct. Do not expose sampler uniforms as plain user
  structs in metadata or diagnostics.
- **Q13:** Required feature set: `Texture2D` logical type, binding specs,
  uniform descriptor ABI, validation, filetests, `texelFetch`, `texture`
  sampling, nearest/linear filtering, clamp/repeat wrapping, height-one
  optimized palette/gradient support, and `Rgba16Unorm`/`R16Unorm` plus likely
  `Rgb16Unorm`. Deferred: mipmaps/auto-LOD/derivatives, 3D/cube/array textures,
  anisotropic/filter gather/comparison/depth samplers, large sidecar fixtures,
  and `clamp_to_border`. `clamp_to_border` is acknowledged as useful for some
  warp/zoom effects, but v0 leaves it out to avoid border-color state until a
  concrete effect needs it.
- **Q14:** WGSL source input is out of scope for this texture-access design and
  should be handled in a later plan, likely alongside real wgpu backend support.
  The design document should still describe WGSL/wgpu context and mapping so
  the texture model does not paint that future work into a corner.
- **Q15:** `mirror_repeat` should be in the texture roadmap, not merely an
  indefinite future note. `Rgb16Unorm` should remain supported on the CPU path
  because it already exists in `TextureStorageFormat`, with a clear WebGPU
  portability caveat. The v0 texture uniform descriptor should include
  `row_stride` even while textures are tightly packed, because the extra word is
  cheap and future-proofs subviews/non-tight rows.

## Related Domain/Render Context

- In the `lp-domain` worktree, effect examples such as
  `kaleidoscope.effect.toml` declare `uniform sampler2D inputColor` in shader
  source while TOML `[input]` supplies the visual or bus route.
- Transition examples such as `wipe.transition.toml` declare
  `uniform sampler2D inputA` and `uniform sampler2D inputB`; the artifact does
  not carry explicit inputs yet because `Live`/`Playlist` runtime context is
  expected to provide them.
- The `lp-render-mvp` roadmap's Stack/Effect milestone expects a texture
  pipeline that supplies upstream output textures into effects and transitions.
  It explicitly notes that input texture naming is still a convention to
  verify.
- Therefore this texture-access design should define a clear `lp-shader`
  contract: descriptor map in at compile time, texture buffers/descriptors bound
  at runtime, and good diagnostics when shader sampler uniforms and descriptors
  do not agree. lpfx/domain can then expose whatever TOML/context surface best
  feeds that contract.

### Q1: Should the internal model be WGSL-style even while GLSL remains the v0 surface?

Context: WGSL separates texture resources from samplers and can express exact
storage texture formats more directly. GLSL is more familiar and matches the
current filetest corpus, but `sampler2D` hides several decisions we care about:
resource format, filter, wrap, and sampled-vs-storage semantics.

Suggested answer: yes. Keep GLSL as the v0 user-facing/filetest surface, but
design the internal type/binding model around WGSL concepts: texture resource
with known format + dimensions, and sampler policy as a separate compile-time
or binding-time decision.

### Q2: Should v0 support only 2D texture resources?

Context: GLSL/WGSL have 1D, 2D, 3D, cube, and array textures, but LightPlayer's
near-term needs are palettes/gradients and 2D effect/transition inputs. Linear
palettes can be represented as width-by-1 2D textures, which matches common
GLSL ES/WebGL practice.

Suggested answer: yes. V0 resource dimensionality is 2D only. 1D operations are
optimised helpers over width-by-1 textures. 3D textures are deferred for future
color-LUT use cases.

### Q3: What shader surface should express filter and wrap policy?

Context: `texelFetch` is naturally nearest/integer. `texture` implies filtered
normalised sampling in GLSL, while WGSL chooses filtering and wrap via sampler
bindings. On RV32, filter and wrap choices strongly affect per-pixel cost.

Suggested answer: model filter/wrap as sampler policy, selected at compile time
or binding time rather than dynamically per sample. Decide whether GLSL spells
this with custom builtins, layout metadata, or compile configuration.

### Q4: What is the v0 feature slice?

Context: There are three user needs: palette lookup, 2D texel fetch, and 2D
interpolated sampling. `texelFetch` is the smallest foundation, but palette
lookup may be the most product-visible need.

Suggested answer: v0 should include texture descriptors + `texelFetch`; v1 adds
filtered 2D sampling; palette LUT helpers are either v1 or v2 depending on how
much host-side palette baking belongs in this design.

### Q5: Where does palette baking live?

Context: Domain configs currently store palettes as UI-style gradient stops.
Shader-side lookup is much cheaper if those stops are baked into a fixed-size
texture. The bake step is product/domain logic more than shader compiler logic.

Suggested answer: `lp-shader` defines the texture sampling primitive and maybe
a narrow helper contract; palette-stop-to-texture baking belongs in a higher
layer such as `lp-domain`/`lp-engine`, not in `lps-frontend` or LPIR.

### Q6: How should filetests bind texture inputs?

Context: Existing filetests compile through LPVM backends directly and call
functions with arguments/uniforms; they do not use `LpsEngine`. Texture tests
need allocated guest memory with known bytes and descriptor binding.

Suggested answer: extend filetests with texture directives that allocate
backend shared memory, populate bytes, and bind a descriptor/uniform before
calling the test function. Add a smaller number of `lp-shader` engine-level
tests for API integration.

### Q7: Should mipmaps/LOD be in scope?

Context: Mipmaps are useful for zoom-out effects on GPUs, but automatic LOD
depends on derivatives that are expensive or unnatural in the CPU pixel loop.
Manual LOD could be added later if a real effect needs it.

Suggested answer: no for v0. Document mipmaps/auto-LOD as out of scope; leave
manual LOD and host-baked mip chains as future work.


# Scope of Work

Implement the texture filetest fixture and diagnostic foundation from
`docs/roadmaps/2026-04-24-lp-shader-texture-access/m2-texture-filetests.md`.

This milestone extends `lps-filetests` so backend-neutral `.glsl` files can
declare texture binding specs, inline fixture data, and texture-specific
diagnostic expectations. It should make texture resources testable by the
filetest harness, but it does not implement `texelFetch` or `texture` lowering.

In scope:

- Parse `// texture-spec:` directives into `TextureBindingSpec` values keyed by
  sampler uniform name.
- Parse `// texture-data:` directives and inline pixel-grouped fixture rows.
- Encode normalized float and exact hex fixture channels for
  `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
- Allocate backend shared memory for texture fixtures and bind typed
  `LpsTexture2DDescriptor` values before each `// run:`.
- Add positive parser/encoder tests and negative diagnostic filetests for:
  missing texture spec, extra spec, missing runtime fixture, malformed fixture
  data, format mismatch, height-one promise mismatch, and unsupported
  filter/wrap spellings.
- Keep fixture declarations backend-neutral so future wgpu comparison can reuse
  them.

Out of scope:

- wgpu comparison runner.
- Large sidecar image fixtures.
- `texelFetch` / `texture` execution behavior beyond expect-fail markers or
  compile/fixture diagnostics needed while later milestones are incomplete.
- lpfx/domain changes.

# Current State

Milestone 1 is complete. Relevant current behavior:

- `lps-shared` defines `TextureStorageFormat`, `TextureBindingSpec`,
  `TextureFilter`, `TextureWrap`, `TextureShapeHint`,
  `LpsTexture2DDescriptor`, and logical `LpsType::Texture2D`.
- `lps-frontend` rewrites simple `uniform sampler2D name;` declarations for
  Naga, lowers supported 2D sampled texture uniforms to `LpsType::Texture2D`,
  and rejects texture parameters gracefully.
- `lp-shader` validates `CompilePxDesc::textures` against declared
  `sampler2D` uniforms through `compile_px_desc`, but `lps-filetests` does not
  call `lp-shader::compile_px_desc`.
- `LpsValueF32::Texture2D` and `LpsValueQ32::Texture2D` carry typed
  `LpsTexture2DDescriptor` values, and F32/Q32 conversion is a no-op for them.
- Ordinary `set_uniform` / `encode_uniform_write` intentionally rejects
  `Texture2D` slots. Texture binding needs a dedicated path rather than a raw
  `set_uniform("tex", uvec4(...))` stand-in.

`lps-filetests` parser state:

- `lp-shader/lps-filetests/src/parse/test_type.rs` defines `TestFile`,
  `RunDirective`, and `SetUniform`. `RunDirective` already carries
  per-run `set_uniforms`, but there are no texture specs or fixtures.
- `lp-shader/lps-filetests/src/parse/mod.rs` scans line-by-line and accumulates
  pending annotations and `// set_uniform:` directives before attaching them to
  the next `// run:` directive.
- `lp-shader/lps-filetests/src/parse/parse_set_uniform.rs` is a small model for
  directive parsing. Texture parsing can follow this style with a new module.
- `lp-shader/lps-filetests/src/parse/parse_source.rs` filters directives out
  of `glsl_source`; harness-only directives before `// run:` fit the existing
  model.

`lps-filetests` execution state:

- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs` compiles filetests by
  calling `lps_frontend::compile` and `lps_frontend::lower` directly, then
  compiles through LPVM backends. It does not use `lp-shader::LpsEngine`.
- `CompiledShader` owns backend modules and can expose `module_sig()`;
  `FiletestInstance` wraps per-run backend instances.
- `FiletestInstance::set_uniform` forwards to each backend's `LpvmInstance`.
  All backend implementations route through `lpvm::encode_uniform_write`, which
  rejects `Texture2D`.
- Backend engines own shared memory through `LpvmEngine::memory()`, but
  `CompiledShader` currently drops engine handles after compilation. Texture
  fixture allocation needs either engine ownership retained in compiled artifacts
  or a lower-level per-backend allocation hook.
- `lpvm::LpvmBuffer` exposes host pointer, guest base, and unsafe read/write
  helpers; this is enough to fill fixture bytes once allocated.

Diagnostics state:

- `// test error` files run through `lp-shader/lps-filetests/src/test_error`.
  That path parses with `lps-frontend`, lowers, then compiles with Cranelift.
  It does not currently know about texture spec directives, because those are
  filetest-level metadata rather than GLSL source.
- Existing error expectations match `// expected-error` message substrings and
  optional codes.
- Run directives can be marked unsupported/unimplemented by annotations, and
  legacy `[expect-fail]` becomes an `@unimplemented` annotation.

Constraints:

- Keep shader compile/execute paths no_std-capable. `lps-filetests` itself is a
  host/std test harness, but changes to `lpvm`, `lps-shared`, or `lps-frontend`
  must not gate embedded compiler behavior behind `std`.
- Do not expose texture descriptors as `uvec4` or public fake struct fields.
- Keep M2 focused on filetest fixture grammar, encoding, validation, and
  binding. Sampling behavior belongs to later milestones.

# Questions That Need To Be Answered

Answers recorded:

- Q6: Exact hex fixture channel width is format-dependent. For M2, all supported
  fixture formats are unorm16, so exact hex channels are 4-digit `u16` values.
  If unorm8 formats are added later, exact hex width should follow the storage
  channel size.
- Q7: Normal typed uniform writes should accept `LpsValueF32::Texture2D` and
  `LpsValueQ32::Texture2D`. Keep guardrails by rejecting raw `UVec4`
  descriptor stand-ins and rejecting opaque subpaths like `tex.ptr`.

## Confirmation-Style Questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Should texture specs be file-level rather than attached to each `// run:`? | Compile-time texture binding specs match shader metadata, not individual runs. | Yes. |
| Q2 | Should texture fixture data be file-level and available to all runs in the file? | M2 examples use one fixture declaration before runs; per-run variation can be added later if needed. | Yes. |
| Q3 | Should `shape=2d` parse as `TextureShapeHint::General2D` and `shape=height-one` (plus maybe `height_one`) parse as `HeightOne`? | Design names are Rust-centric; directives should be short and readable. | Yes. |
| Q4 | Should `wrap=clamp` be accepted as shorthand for both axes `ClampToEdge`, with optional `wrap_x=` / `wrap_y=` reserved or supported for axis-specific cases? | Milestone example uses `wrap=clamp`, but `TextureBindingSpec` stores two axes. | Yes: implement `wrap=` for both axes and support `wrap_x=`/`wrap_y=` if cheap. |
| Q5 | Should texture fixture channels encode normalized floats with the same canonical storage conversion used by render output tests? | Design says float fixture channels are converted through canonical storage conversion. | Yes. |
| Q6 | Should exact hex fixture channels be fixed-width 4 hex digits per `u16` channel, case-insensitive? | Storage formats are all unorm16 today. If/when unorm8 formats are added, exact hex width should follow that storage channel size. | Yes for now; format-dependent longer term. |
| Q7 | Should M2 allow normal typed uniform writes for `Texture2D` values? | `set_uniform` currently rejects texture slots, but `Texture2D` is now a first-class typed `LpsValue`, and filetest fixtures can construct typed descriptors internally. | Yes; allow typed `LpsValueF32/Q32::Texture2D`, still reject raw `UVec4` stand-ins and subpaths. |

## Discussion-Style Questions

### Q7: How should filetests bind texture descriptors?

Current state:

- `LpsValueF32::Texture2D` and `LpsValueQ32::Texture2D` are first-class typed
  ABI values.
- `lpvm::encode_uniform_write` and `encode_uniform_write_q32` currently reject
  `LpsType::Texture2D` paths. This was intended as a guardrail, but it is now
  stricter than the typed value model requires.
- `lps-filetests` currently only has `// set_uniform:`. It forwards through
  `FiletestInstance::set_uniform`, which forwards to backend
  `LpvmInstance::set_uniform`.
- M2 needs the harness to write a typed `LpsTexture2DDescriptor` into the
  texture uniform slot after allocating and filling fixture memory.
- `CompilePxDesc::textures` is a compile-time spec map
  (`TextureBindingSpec`), not a runtime descriptor/resource map. Runtime
  descriptor values are still uniforms written after instantiation.

Possible approaches:

1. Relax `set_uniform` to accept `LpsValueF32::Texture2D` /
   `LpsValueQ32::Texture2D`, while still rejecting `UVec4` stand-ins and
   subpaths like `tex.ptr`.
2. Keep `set_uniform` guarded and add a dedicated texture binding helper.
   Example: `FiletestInstance::bind_texture2d(name, descriptor)` backed by a
   shared `lpvm::encode_texture2d_uniform_binding(...)` helper that only
   accepts whole `Texture2D` uniform paths and typed descriptors.
3. Add a higher-level runtime API outside `set_uniform`, perhaps on
   `LpvmInstance`, but keep the low-level byte encoder private to backends.

Suggested answer:

Use approach 1 for M2: make the normal typed uniform methods accept
`LpsValueF32::Texture2D` and `LpsValueQ32::Texture2D`. This matches the
first-class value model and lets filetest fixtures bind textures by internally
constructing a typed descriptor and calling `set_uniform`. Keep the safety
properties by rejecting `UVec4` descriptor-shaped stand-ins and rejecting
subpaths like `tex.ptr` because `Texture2D` is an opaque logical type.


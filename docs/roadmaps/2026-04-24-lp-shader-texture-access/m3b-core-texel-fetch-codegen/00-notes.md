# Scope of Work

Plan Milestone 3b:
`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen.md`.

This milestone replaces the M3a `texelFetch` placeholder with real LPIR data
path lowering for supported GLSL `texelFetch(sampler2D, ivec2, 0)` calls.

In scope:

- Lower M3a-recognized Naga `Expression::ImageLoad` / GLSL `texelFetch` into
  LPIR instead of returning the placeholder diagnostic.
- Resolve the direct `Texture2D` uniform sampler and matching
  `TextureBindingSpec` using the M3a lowering context.
- Load the four texture descriptor lanes from the uniform VMContext ABI:
  `ptr`, `width`, `height`, and `row_stride`.
- Lower integer coordinate expressions for the `ivec2` coordinate operand.
- Implement the chosen v0 out-of-range coordinate policy.
- Add a compiler option that controls whether `texelFetch` bounds clamps are
  generated. The default must be safe (`clamp-to-edge`); the opt-out is for
  performance measurement and should be explicit.
- Compute byte addresses as `ptr + y * row_stride + x * bytes_per_pixel`,
  honoring descriptor `row_stride` rather than assuming tight rows.
- Emit format-specialized `Load16U` and `Unorm16toF` sequences for:
  - `R16Unorm`,
  - `Rgb16Unorm`,
  - `Rgba16Unorm`.
- Return GLSL-compatible `vec4` values:
  - R: `(r, 0.0, 0.0, 1.0)`,
  - RGB: `(r, g, b, 1.0)`,
  - RGBA: `(r, g, b, a)`.
- Add exact-value texture filetests on the initial validation target and narrow
  unit tests where they make the lowering shape easier to pin down.
- Fix existing backend lowering gaps surfaced by the required `wasm.q32`,
  `rv32n.q32`, and `rv32c.q32` alignment tests when those fixes are necessary
  for existing LPIR ops such as `Load16U`.

Out of scope:

- Runtime validation of host-provided `LpsTextureBuf` / descriptor values.
- Broad backend matrix validation across all LPVM targets.
- Public API helpers for texture binding.
- Normalized-coordinate `texture()` sampling, filtering, wrap modes, mipmaps,
  or any `lod != 0` behavior.
- Adding a dedicated LPIR texture opcode or runtime format switch.

# Current State

M1 and M2 have provided the texture interface and filetest fixture surface:

- `lps-shared` defines `TextureBindingSpec`, `TextureStorageFormat`,
  `TextureFilter`, `TextureWrap`, `TextureShapeHint`, `LpsType::Texture2D`,
  and `LpsTexture2DDescriptor`.
- `TextureStorageFormat::{R16Unorm,Rgb16Unorm,Rgba16Unorm}` exposes
  `bytes_per_pixel()` and `channel_count()`.
- `LpsType::Texture2D` lowers through the VMContext uniform ABI as four `i32`
  lanes.
- `lps-filetests` parses `// texture-spec:` and `// texture-data:`, encodes
  tightly packed little-endian unorm16 fixtures, allocates shared memory, and
  binds a `LpsValueF32::Texture2D` descriptor before each `// run:`.

M3a is in progress and appears mostly wired:

- `lps_frontend::LowerOptions` carries compile-time texture specs into lowering.
- `LpsModuleSig` retains validated texture specs for future runtime validation.
- `LowerCtx` stores `texture_specs`.
- `lower_expr.rs` routes Naga `Expression::ImageLoad` with `level: Some(_)` to
  `lower_texture::lower_image_load_texel_fetch`.
- `lower_texture.rs` resolves direct uniform `Texture2D` operands, validates a
  matching spec, rejects unsupported LOD forms, and currently returns:
  `texelFetch for texture uniform \`...\` recognized; data path is implemented in M3b`.
- Texture diagnostic filetests already cover missing spec, dynamic LOD, nonzero
  LOD, and the M3b placeholder.

Relevant LPIR/backend state:

- LPIR already has `Load`, `Load16U`, integer arithmetic/comparison ops,
  `Select`, `FconstF32`, `IconstI32`, and `Unorm16toF`.
- `Load16U` and `Unorm16toF` are implemented across interpreter, Cranelift,
  wasm, and native RV32 paths.
- `lpir::CompilerConfig` already has filetest-visible `compile-opt` plumbing
  through `CompilerConfig::apply`, but frontend lowering currently does not
  receive `CompilerConfig`; M3b needs to thread a texture lowering subset into
  `lps_frontend::LowerOptions`.
- `lower_expr.rs::load_lps_value_from_vmctx_with_base` already knows how to load
  a `Texture2D` descriptor as four `i32` lanes from `base + offset`.
- Existing lowering helpers already emit constants, dynamic uniform array
  address math, and `Select`-based branchless values.

Constraints:

- `lps-frontend` is `no_std + alloc`; M3b must not add `std` dependencies to the
  compile/lower path.
- Keep the M3a strict operand contract: direct uniform sampler only.
- Keep format dispatch compile-time through `TextureBindingSpec`; do not emit a
  runtime format switch.
- Diagnostics should name sampler uniforms and texture operations, not expose
  descriptor internals as user-facing concepts.

# Questions That Need To Be Answered

## Confirmation-Style Questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Should M3b keep all texture data-path lowering in `lps-frontend/src/lower_texture.rs`? | M3a already created this module for texture-specific contract logic. | Yes: replace the placeholder there and add helper functions at the bottom. |
| Q2 | Should descriptor lane order be hard-coded as `ptr=0`, `width=4`, `height=8`, `row_stride=12` at the lowering helper boundary? | The ABI is fixed by `LpsTexture2DDescriptor` and `LpsType::Texture2D` layout. | Yes, preferably via named constants local to `lower_texture.rs`. |
| Q3 | Should M3b initially validate exact texture reads on `rv32n.q32` filetests? | `rv32n.q32` exercises the default on-device native RV32 path and M2 fixtures already bind through shared memory. | Yes: use `rv32n.q32` as the required behavior target, with optional host/wasm expansion in M3c. |
| Q4 | Should the existing M3a placeholder filetest be converted into a positive run test instead of kept as an expected error? | Once M3b lands, that diagnostic should disappear. | Yes: replace or rename it to a positive exact-value test. |
| Q5 | Should channel conversion use LPIR `Unorm16toF` directly after each `Load16U`? | The milestone requires consistency with existing Q32/native-float unorm behavior. | Yes: do not hand-roll conversion in frontend lowering. |

## Discussion-Style Questions

## Q6: What should v0 out-of-range `texelFetch` coordinates do?

GLSL `texelFetch` with out-of-range coordinates is undefined. The roadmap says
M3b should implement the v0 policy chosen during planning, but that choice is
not yet recorded. M4 will add filtered sampling and wrap modes later; M3b can
either keep texelFetch strict/simple or align early with a clamp-style policy.

Suggested answer:

- Use **clamp-to-edge** for v0 `texelFetch`: clamp `x` into
  `[0, width - 1]` and `y` into `[0, height - 1]` before address math.
- Reason: it avoids unsafe out-of-bounds reads from shared memory in tests and
  runtime, gives deterministic behavior, and matches the only safe subset of
  the existing `wrap=clamp` fixture policy.
- Revisit wrap/repeat/mirror only in M4 for normalized `texture()` sampling;
  do not make `texelFetch` dispatch on `TextureWrap` in M3b.

# Answers

- Q1: Keep M3b texture data-path lowering in `lps-frontend/src/lower_texture.rs`.
  M3a already owns the texture-specific lowering contract there; split later if
  M4 filtered sampling makes the module too broad.
- Q2: Use named descriptor lane offsets in `lower_texture.rs` for the fixed
  `LpsTexture2DDescriptor` ABI: `ptr=0`, `width=4`, `height=8`,
  `row_stride=12`. Named constants keep ABI use explicit and auditable.
- Q3: Target the three mainline Q32 filetest backends for M3b exact-value
  coverage: `wasm.q32`, `rv32n.q32` (`lpvm-native`, the product backend), and
  `rv32c.q32` (Cranelift RV32). This should be mostly filetest coverage because
  the needed LPIR ops already exist on those backends.
- Q4: Convert/rename the M3a placeholder expected-error filetest into a positive
  M3b exact-value run test. It already contains the minimal valid supported
  `texelFetch(inputColor, ivec2(0, 0), 0)` shape; M3b should add fixture data,
  run it on `wasm.q32`, `rv32n.q32`, and `rv32c.q32`, and assert sampled
  channel values.
- Q5: Use LPIR `Load16U` for each stored unorm16 channel, followed directly by
  LPIR `Unorm16toF`. Do not hand-roll conversion in frontend lowering. Emit
  missing RGB channels as `FconstF32 0.0` and default alpha as `FconstF32 1.0`.
- Q6: Clamp out-of-range `texelFetch` integer coordinates to edge by default:
  `x` clamps to `[0, width - 1]`, and `y` clamps to `[0, height - 1]`. This is
  a deterministic memory-safety policy for lower-level integer-coordinate
  fetches, not sampler wrapping behavior; `TextureWrap` remains for M4
  `texture()` sampling. Add the compiler option in M3b now, defaulting to safe
  clamping, with an explicit unchecked/fast mode to disable clamp generation for
  performance measurement.

# Notes

- The compiler option should default to memory-safe clamp generation because
  arbitrary out-of-bounds shared-memory reads are not acceptable general
  behavior. Suggested shape: `CompilerConfig::texture.texel_fetch_bounds =
  ClampToEdge | Unchecked`, with filetest key
  `texture.texel_fetch_bounds=clamp-to-edge|unchecked`.
- M3b filetests should keep targeting `wasm.q32`, `rv32n.q32`, and `rv32c.q32`.
  If those aligned tests expose a backend gap for an existing LPIR op, fixing
  that backend gap is a reasonable part of M3b rather than deferring the target.

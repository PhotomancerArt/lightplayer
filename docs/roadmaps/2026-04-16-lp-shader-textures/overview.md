# lp-shader Texture System — Overview

## Motivation

Textures and pixel loops are currently implemented ad-hoc by each consumer
(lp-engine, lpfx-cpu) with duplicated code for format conversion, per-pixel
iteration, and Q32 -> u16 output writes. The shader itself has no concept of
textures -- it's called as a function, returns a vec4, and the caller does
the rest.

This roadmap introduces textures as a first-class concept in lp-shader,
formalizes the fragment shader contract (standard GLSL `out vec4; void main`),
and moves the pixel loop into lp-shader where backends can optimize it.

## Key Design Decisions

### 1. New `lp-shader/lp-shader` crate

High-level shader API that wraps the frontend + a backend engine. Generic
over `LpvmEngine`. Consumers (lpfx, lp-engine) use this instead of
hand-wiring `lps-frontend` + `lpvm-cranelift`.

The lower-level crates stay as they are: `lpvm` (runtime traits, VmContext),
`lps-shared` (types, layout), `lpir` (pure IR), `lps-frontend` (GLSL parse).
Backend crates (`lpvm-cranelift`, `lpvm-native`) keep implementing
`LpvmEngine`.

```
lps-shared          (types, layout, TextureStorageFormat)
    |
    +--- lpir       (IR definition, pure)
    |
    +--- lpvm       (runtime traits, VmContext, memory)
    |      |
    |      +--- lpvm-cranelift  (backend)
    |      +--- lpvm-native     (backend)
    |
    +--- lps-frontend  (GLSL -> LPIR via naga)
    |
    +--- lp-shader  (NEW: high-level API)
             depends on: lps-frontend, lpvm, lps-shared, lpir
             provides: LpsShaderEngine, compile_frag, FragModule,
                       render_frame, texture buffer types
```

### Host execution backend

**Wasmtime** (`lpvm-wasm`, e.g. `WasmLpvmEngine`) is the **host execution
backend** for new work: `lp-shader` unit tests, future `lp-cli` / authoring
tools, and (after M4) `lp-engine` / `lpfx-cpu` CPU paths should run
shaders through Wasmtime for deterministic, per-instance isolation and
32-bit guest pointers consistent with RV32, the emulator, and the browser.

The in-process **`lpvm-cranelift` JIT remains in the tree** but is
**deprecated for new work**; it is still wired into `lp-engine` and
`lpfx-cpu` until M4 completes. **Removal is a separate, later task.** A
small **Phase 2 handwritten smoke test** in `lpvm-cranelift` stays as a
regression guard for the JIT `call_render_texture` / trait shape.

### 2. Fragment shader as a first-class concept

`compile_frag` produces a `FragModule` / `FragInstance` with `render_frame`,
distinct from the existing generic `compile` -> `call(name, args)` path.

The existing generic path stays for filetests, expression evaluation, and
any non-fragment use case. `lpvm` remains shader-agnostic.

### 3. Standard GLSL fragment shader contract

The shader contract moves from the current function-argument style:

```glsl
vec4 render(vec2 fragCoord, vec2 outputSize, float time)
```

To standard GLSL fragment shader style:

```glsl
out vec4 fragColor;

void main() {
    vec2 fragCoord = gl_FragCoord.xy;
    // ... compute color ...
    fragColor = vec4(...);
}
```

`gl_FragCoord` is a built-in (injected as a uniform on CPU, native built-in
on GPU). `outputSize` and `time` are regular uniforms set by the runtime
before rendering.

Consumers like lpfx that want to support the old `render(fragCoord,
outputSize, time)` user-facing API can generate a GLSL bootstrap wrapper:

```glsl
uniform vec2 outputSize;
uniform float time;
out vec4 fragColor;
// ... user code with render() definition ...
void main() { fragColor = render(gl_FragCoord.xy, outputSize, time); }
```

### 4. Output format bound at compile time

`compile_frag` takes a `FragOutputDesc` describing the output texture format.
This enables the compiler to emit format-specific conversion code (Q32 ->
unorm16) directly, and in the future, a fully-inlined synthetic
`__render_frame` function that fuses the pixel loop with the shader and
format conversion (enabled by the LPIR inliner from the separate inliner
roadmap).

First implementation uses the format at runtime (backend pixel loop with
format-aware writes). The synthetic LPIR function is a future optimization
that slots in without API changes.

### 5. Texture storage format

`TextureStorageFormat` in `lps-shared`. Unorm16 is the natural format:
the lower 16 bits of a clamped Q16.16 value are a unorm16 value (to within
1 LSB). The conversion is `(clamped_q32 * 65535) >> 16`.

Single format for now:
- `Rgba16Unorm` -- 8 bytes/pixel, RGBA, default for CPU rendering

The GPU path (`lpfx-gpu`) uses `wgpu::TextureFormat::Rgba16Float` directly
at the wgpu API level -- it doesn't need an lp-shader enum entry for a
format the CPU path can't produce (no f16 in LPIR/Q32).

Future variants (Rgb16Unorm for embedded memory, R16Unorm for data textures)
are added when there's a concrete consumer.

### 6. Output globals (new compiler feature)

`lps-frontend` switches from `ShaderStage::Vertex` to `ShaderStage::Fragment`.
`AddressSpace::Output` globals (naga's representation of `out vec4 fragColor`)
are handled and added to `LpsModuleSig` as a new field alongside
`uniforms_type` / `globals_type`.

The runtime reads output globals from vmctx after each `main()` invocation
to get the pixel color.

### 7. Texture reads (later milestone)

Texture table in vmctx (array of base pointers + dimensions + format).
`texelFetch` builtin for integer-coordinate lookup with format-aware load.
`sampler2D` type support in the frontend. Enables palette lookups, multi-pass
effects, data textures.

## Alternatives Considered

- **Merge lps-shared + lpvm**: rejected. The split (types vs runtime) is
  reasonable. A new layer on top (lp-shader) ties them together without
  restructuring existing crates.

- **Keep function-argument shader contract**: rejected. The current
  `render(fragCoord, outputSize, time)` API exists because we didn't have
  uniforms. Now we do. Standard GLSL fragment shader contract enables GPU
  portability, output globals, and cleaner separation of runtime-provided
  values from user inputs.

- **Dynamic format dispatch at render time**: rejected as the default. Output
  format at compile time enables the compiler to emit format-specific writes
  and eventually fully-inlined render functions. A slow path with runtime
  format dispatch can exist for testing.

## Risks

- **ShaderStage::Fragment in naga**: switching from Vertex to Fragment may
  change how naga handles entry points, built-ins, and globals. Needs careful
  testing. Existing filetests may need updates if naga's behavior differs
  between stages.

- **Output globals in vmctx**: output globals need to be readable by the
  runtime after `main()` returns. They live in the globals region of vmctx
  (like private globals today), with known offsets from `LpsModuleSig`.

- **Migration scope**: lp-engine and lpfx-cpu both need to adopt the new API.
  The old paths can coexist during migration.

## Milestones

- **M2.0 — `render_frame` via synthetic `__render_texture`** — ✅ complete.
  Implementation plan: [`docs/plans/2026-04-17-lp-shader-textures-stage-v/`](../../plans/2026-04-17-lp-shader-textures-stage-v/).
- **M4a — Pixel-loop migration** ([`m4a-pixel-loop-migration.md`](./m4a-pixel-loop-migration.md)) — ✅ complete.
  Implementation plan: [`docs/plans-old/2026-04-19-m4a-pixel-loop-migration.md`](../../plans-old/2026-04-19-m4a-pixel-loop-migration.md).
  Moved `lp-engine`'s hand-rolled per-pixel loops into `LpsPxShader::render_frame`.
  Phase 4 (`gfx/native_jit.rs`) was rolled into Phase 3 in implementation;
  both gfx wrappers now hold an `LpsEngine<…>` and delegate `render_frame`.
- **M4b — Host backend swap** ([`m4b-host-backend-swap.md`](./m4b-host-backend-swap.md)).
  Swap `lp-engine`'s host backend from `lpvm-cranelift` to Wasmtime via `lpvm-wasm`.
  Mechanical after M4a; firmware (RV32) path unaffected.
- **M4c — `lpfx-cpu` migration** ([`m4c-lpfx-cpu-migration.md`](./m4c-lpfx-cpu-migration.md)).
  Same migration for the standalone `lpfx-cpu` consumer; deletes its
  duplicate compile/render pipeline and consolidates texture types.
- **M3 — Texture reads** ([`m3-texture-reads.md`](./m3-texture-reads.md)).
  `texelFetch` builtin and `sampler2D` support. Deferred behind M4a/b/c
  because there is no current consumer that produces textures another
  shader could sample; revisit once M4 is done and multi-pass shaders
  have a real use case.

## Dependencies

- Globals/uniforms infrastructure (done)
- LPIR inliner (separate roadmap, benefits but doesn't block this work)

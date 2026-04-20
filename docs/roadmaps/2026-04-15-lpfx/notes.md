# LPFX — Effect Module System Notes

## Product Context

LightPlayer is an embedded GLSL JIT system. The web app (lightplayer.app)
shows a live rendering of an LED strip/fixture running a pattern, with knobs
to tweak and an AI chat to modify effects in real-time. A gallery of pre-made
2D visualizations lets users click and immediately see them running. A connect
button flashes an ESP32 and pipes the effect to real hardware.

## Scope

Introduce a first-class **effect module** concept that replaces the current
separate shader + texture node pair. An effect module is a self-contained,
shareable, resolution-independent unit that:

- Bundles shader code, texture definitions, input definitions, and metadata
- Can be run on lp-shader (CPU JIT) or a real GPU (WebGL/WebGPU)
- Is the thing that shows up in the effects picker / gallery
- Is distributable as a directory (zip, git repo, etc.)

## Current State

### What exists

- **NodeKind**: `Texture`, `Shader`, `Output`, `Fixture` — four top-level
  node types discovered from filesystem suffixes.
- **ShaderConfig**: `glsl_path`, `texture_spec` (points at a texture node),
  `render_order`, `glsl_opts` (Q32 modes).
- **TextureConfig**: `width`, `height` only.
- **ShaderRuntime**: loads GLSL, calls `LpGraphics::compile_shader`, holds
  `Box<dyn LpShader>`, resolves target texture handle.
- **Render model**: `shader.render(texture, time)` per frame. Multiple
  shaders target one texture via `render_order`.
- **Uniforms/globals**: just landed — `set_uniform`, `__shader_init`,
  globals snapshot/reset lifecycle on `LpvmInstance`.
- **Builtin shader library** (`lps-builtins`): noise, hash, color-space
  functions currently using the `lpfx_` GLSL prefix. Will be renamed to
  free `lpfx` for the effect module system (see naming section).
- **lp-app/web-demo**: exists but minimal.
- **fw-wasm**: does not exist yet (device stand-in for in-browser preview).

### What doesn't exist

- No "effect" or "program" as a first-class entity.
- No module manifest format (inputs, suggested resolution, metadata).
- No multi-pass / multi-texture composition within a single effect.
- No palette system.
- No GPU rendering path (WebGL/WebGPU).
- No effect gallery / picker infrastructure.
- No AI-driven effect editing workflow.

## Naming Decisions

### User-facing: "effect". Code-facing: "Fx".

- UI says "effects picker", "browse effects", "edit effect".
- Rust types: `FxEngine`, `FxModule`, `FxInstance`.
- Directory suffix: `.fx` (e.g. `rainbow-wave.fx/`).
- Manifest: `fx.toml` (inside the `.fx` directory).
- Note: lp-engine currently uses JSON (`node.json`) for node config.
  Reconciliation TBD — lpfx uses TOML for human-authored manifests;
  lp-engine may adopt TOML or lpfx may accept either format.

### Architecture mirrors lpvm

| lpvm             | lpfx (new)       | Role                                    |
|------------------|------------------|-----------------------------------------|
| `LpvmEngine`     | `FxEngine`       | Config, loading modules                 |
| `LpvmModule`     | `FxModule`       | Loaded resources, not yet runnable      |
| `LpvmInstance`   | `FxInstance`     | Running module with state               |

### Repo structure

```
lpfx/                      # top-level directory (like lp-core, lp-shader, lp-fw)
  lpfx/                    # core types: FxModule, FxInstance, manifest parsing, input defs
  lpfx-cpu/                # (or lpfx-lpvm) runs effects via lp-shader CPU JIT
  lpfx-gpu/                # runs effects via WebGL/WebGPU (naga → WGSL)
  lpfx-web/                # wasm bindings for browser usage
  lpfx-cli/                # standalone CLI tool for running/testing effects
  lpfx-demo/               # demo effects / example gallery
```

### System map after renaming

```
lp-app   — web app for interacting with an lp-server
lp-core  — engine for running projects, handling inputs, server/client
lp-fw    — firmware that runs on device, hosts an lp-server
lp-shader — GLSL compiler: frontend → LPIR → codegen (lpvm)
lpfx     — runnable effects, independent from how they are run
```

`lpfx` depends on `lp-shader` for CPU execution but does not depend on
`lp-core` or `lp-fw`. You can run an effect without LightPlayer.

### Renaming the builtin shader library

The existing `lpfx_*` GLSL functions (noise, hash, color-space, etc.) in
`lps-builtins` need a new prefix to free the `lpfx` namespace.

**Decided: `lpfx`** ("LightPlayer functions"). `lpfx_hash`, `lpfx_fbm`,
`lpfx_hsv2rgb`. LPIR module `@lpfx::`. Grep-able, easy to rename again
later if needed. Separate prerequisite task (~10 categories of references,
all mechanical).

## Questions

### Q1: Trait design — FxEngine / FxModule / FxInstance

**Status: Decided.** See dedicated section below for full design.

### Q2: Module manifest and type system

**Status: Decided (MVP scope).** See "Module Design Discussion" section below.

### Q3: How to handle palettes?

**Status: Decided (high level).** Palette is a semantic input type. Under
the hood it's a texture (1D, e.g. 32x1). The manifest declares it as
`type = "Palette"`, the UI shows a palette/gradient editor, and the shader
reads it as a `sampler2D`. Details of palette representation (color stops,
interpolation mode, color space) TBD when we implement it.

### Q4: State (multi-pass, persistent textures)

**Status: Deferred.** See `future-work.md`. MVP is single-pass pure
functions. Manifest format will extend with `[[pass]]` later.

### Q5: GPU rendering path

**Status: Decided.** GLSL → naga → WGSL → WebGPU via `wgpu` crate.
All Rust, no npm. Q32 differences accepted for preview. See `m2-gpu.md`.

### Q6: Module directory structure

**Status: Decided.**
```
rainbow-noise.fx/
  fx.toml          # manifest (required)
  main.glsl        # pixel shader (required)
```

### Q7: Relationship to existing node graph

**Status: Deferred.** See `future-work.md`. lpfx is independent of
lp-core. Future `Effect` node kind wraps `FxModule` + `FxInstance`.

## Milestones

| Milestone | Title              | Dependencies | Status  |
|-----------|--------------------|--------------|---------|
| M0        | Scaffold + effect  | —            | Pending |
| M1        | CPU rendering path | M0           | Pending |
| M2        | GPU rendering path | M0           | Pending |
| M3        | Preview web app    | M1, M2       | Pending |

```
M0 (scaffold + effect) → M1 (CPU) ─┐
                         M2 (GPU) ─┤→ M3 (preview app)
```

M1 and M2 can run in parallel after M0. M3 needs both.

See also: `future-work.md` for deferred items.

## Trait Design Discussion

### Reference: current lpvm stack

```rust
// LpvmEngine: holds compile options. Compiles LPIR → runnable module.
pub trait LpvmEngine {
    type Module: LpvmModule;
    type Error: Display;
    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
    fn memory(&self) -> &dyn LpvmMemory;
}

// LpvmModule: compiled code, not yet running. Can create instances.
pub trait LpvmModule {
    type Instance: LpvmInstance;
    type Error: Display;
    fn signatures(&self) -> &LpsModuleSig;
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;
}

// LpvmInstance: execution state. Mutable. Calls functions.
pub trait LpvmInstance {
    type Error: Display;
    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error>;
    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error>;
    fn write_vmctx_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error>;
}
```

### Reference: current lp-engine consumption

```rust
// LpGraphics: backend-agnostic shader compilation (Cranelift, native JIT, wasm)
pub trait LpGraphics: Send + Sync {
    fn compile_shader(&self, source: &str, options: &ShaderCompileOptions)
        -> Result<Box<dyn LpShader>, Error>;
}

// LpShader: a compiled, runnable shader. Bundles LpvmModule + Instance internally.
pub trait LpShader: Send + Sync {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error>;
    fn has_render(&self) -> bool;
}

// ShaderRuntime: lp-engine node. Holds config, LpGraphics arc, compiled shader,
// target texture handle. Handles init, render, file-change recompilation.
pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    graphics: Arc<dyn LpGraphics>,
    shader: Option<Box<dyn LpShader>>,
    texture_handle: Option<TextureHandle>,
    // ...
}
```

### Proposed lpfx stack — decided shape

Three-layer design mirroring lpvm. Form follows function.

```
FxEngine    — config + backend. Loads modules and instantiates them.
FxModule    — parsed manifest + GLSL source + input defs. Not compiled. Cheap.
FxInstance  — compiled shaders + allocated textures + input state. Renderable.
```

#### FxEngine

Backend-specific. Internally holds an `LpvmEngine` (for CPU path) or a GPU
context (for WebGL/WebGPU path). Responsible for:

- Loading modules from disk (`engine.load(path) → FxModule`)
- Compiling and instantiating (`engine.instantiate(&module, resolution) → FxInstance`)

`instantiate` lives on `FxEngine`, not `FxModule`, because the module is
backend-agnostic — it doesn't know whether it'll run on CPU or GPU.

```rust
impl FxEngine {
    fn load(&self, path: &Path) -> Result<FxModule, Error>;
    fn instantiate(&self, module: &FxModule, resolution: (u32, u32))
        -> Result<FxInstance, Error>;
}
```

#### FxModule

Parsed manifest + GLSL source. Backend-agnostic. Cheaply clonable. No
compiled code. The gallery can hold 50 of these without compiling anything.

```rust
impl FxModule {
    fn manifest(&self) -> &FxManifest;
    fn inputs(&self) -> &[FxInputDef];
    fn glsl_source(&self) -> &str;  // MVP: single shader
}
```

#### FxInstance

A running effect. Backend-specific (type-erased behind a trait or generic).
Internally holds one or more `LpvmModule` + `LpvmInstance` (for CPU path).
Owns the pixel loop and the uniform-setting lifecycle that currently lives
in `CraneliftShader::render`.

```rust
impl FxInstance {
    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Error>;
    fn render(&mut self, time: f32) -> Result<(), Error>;
    fn output(&self) -> &Texture;
}
```

#### Decisions

1. **Compilation is deferred to instantiation.** `load()` parses the
   manifest and reads source files. `instantiate()` compiles shaders and
   allocates textures. This keeps modules cheap and inspectable.

2. **FxInstance exists as a separate type.** Reasons:
   - Matches the proven lpvm Engine → Module → Instance pattern
   - Clear separation: "what is this effect" vs "a running copy of it"
   - Multiple instances from one module (different input values)
   - Module is `Send + Sync` and backend-agnostic; instance is mutable
     and backend-specific

3. **Updates work by replacement.** When a file changes (shader edited,
   manifest updated), the consumer (lp-engine's Effect node) re-loads the
   `FxModule` and creates a new `FxInstance`. The old instance is dropped.
   Same pattern as `ShaderRuntime` today — it drops the old `Box<dyn
   LpShader>` and recompiles.

4. **Internal composition**: MVP FxInstance holds one LpvmModule + one
   LpvmInstance (single-pass). Future multi-pass effects hold several.
   The FxInstance owns the render loop (iterates pixels, sets uniforms,
   calls the compiled shader).

5. **Primary consumer**: `lp-engine` will have an `Effect` node kind
   whose runtime holds an `FxModule` + `FxInstance`. File changes trigger
   reload → re-instantiate, same as `ShaderRuntime.handle_fs_change`.

#### Lifecycle in lp-engine

```
1. engine.load("rainbow-wave.fx/") → FxModule
   (parses fx.toml, reads GLSL, validates config/input defs)

2. engine.instantiate(&module, resolution) → FxInstance
   (compiles shaders via LpvmEngine, allocates output texture)

3. per frame:
   instance.set_config("speed", FxValue::F32(2.0))     // user knob
   instance.set_input("audio", &audio_data)             // runtime source
   instance.render(time) → fills output texture

4. on file change:
   re-run step 1 → new FxModule
   re-run step 2 → new FxInstance (old one dropped)
```

## Module Design Discussion

### Core insight: everything is an input

At the data level, everything a shader reads from outside is just an input.
The distinction between "config" (user knobs) and "system sources" (audio,
touch) is a **presentation/UX concern**, not a data model concern.

A `speed: f32` is a float. It could come from a UI slider, a potentiometer
on a GPIO pin, a DMX channel, a MIDI CC, an MQTT topic, or another effect's
output. The shader doesn't know or care.

**Design**: one `[input.*]` table. Each input has a **type** (data shape)
and a **role** (UX hint about the author's intent). The role tells the UI
what widget to show by default, but doesn't restrict what can be bound.

Roles discussed so far: `"config"`, `"audio"`, `"video"`, `"touch"`.
The binding system (DMX, MIDI, potentiometer, etc.) is a lp-core / project
concern — lpfx just declares inputs and accepts values.

### Type system

Two naming conventions:

- **Primitives** (lowercase, Rust/WGSL style): `f32`, `i32`, `u32`, `bool`,
  `vec2`, `vec3`, `vec4`
- **Semantic types** (PascalCase): `Color`, `Palette`, `AudioFft`,
  `AudioLevel`, `AudioBeat`, `Touch`

WGSL/Rust-style type names preferred over GLSL. Unambiguous, shorter,
industry direction.

PascalCase for semantic types because they carry meaning beyond data layout:
they imply UI widget, conversion pipeline, runtime behavior.

### MVP input types

| Manifest type | Shader uniform | Default role | UI widget     |
|---------------|----------------|--------------|---------------|
| `f32`         | `float`        | `config`     | slider        |
| `i32`         | `int`          | `config`     | slider        |
| `bool`        | `bool`         | `config`     | toggle        |
| `Color`       | `vec3`         | `config`     | color picker  |
| `Palette`     | `sampler2D`    | `config`     | palette editor|

### Future input types (not MVP)

Distinct types per representation — each gets its own options struct
(e.g. `AudioFftOptions`).

| Manifest type | Shader representation  | Default role | Notes              |
|---------------|------------------------|--------------|--------------------|
| `AudioFft`    | uniform array / 1D tex | `audio`      | frequency spectrum |
| `AudioLevel`  | float uniforms         | `audio`      | peak + RMS, cheap  |
| `AudioBeat`   | float uniform          | `audio`      | beat pulse + decay |
| `Touch`       | uniform struct/array   | `touch`      | finger positions   |

#### AudioFft options

```toml
[input.spectrum]
type = "AudioFft"
bands = 32
scale = "octave"          # octave, linear, mel, bark
range_hz = [20, 20000]
smoothing_ms = 50
```

#### AudioLevel options

```toml
[input.level]
type = "AudioLevel"
# provides: peak (f32), rms (f32)
```

#### AudioBeat options

```toml
[input.beat]
type = "AudioBeat"
decay_ms = 200            # pulse decay time
```

### Color space awareness

Colors must not be untyped. A `Color` input carries its color space.

```toml
[input.base_color]
type = "Color"
space = "srgb"           # default if omitted
default = [0.2, 0.5, 1.0]
```

Supported spaces (eventual, not all MVP): srgb, linear, hsl, hsv, oklch,
oklab. The runtime converts from manifest space to what the shader expects
(likely linear RGB for math).

Palette inherits the same concern — all stops share one space.

### Output texture

MVP: always RGBA16, implicit. No `[output]` section in the manifest.
All color values 0–65535. Matches the existing lp-engine texture format.

Future: `[output]` section with depth/format hints. Not now.

### Shader

MVP: always `main.glsl`. No `[render]` section in the manifest.
If `main.glsl` is missing from the `.fx` directory, that's a load error.

Future: `[render]` or `[[pass]]` sections for multi-pass/multi-shader.
Not now.

### Convention: unit suffixes on physical values

Fields that accept physical values must have a unit suffix. No unitless
ambiguity.

- `range_hz` — frequency in Hz
- `smoothing_ms`, `decay_ms`, `timeout_ms` — time in milliseconds
- `frequency_hz` — frequency in Hz

Config-role inputs that represent physical values can carry a `unit`
display hint:

```toml
[input.cutoff]
type = "f32"
label = "Cutoff Frequency"
range = [20.0, 20000.0]
unit = "hz"                # UI shows "440 Hz"
default = 440.0
```

### MVP manifest

Minimal: `[meta]`, `[resolution]`, and `[input.*]`. Output is always
RGBA16, shader is always `main.glsl` — both implicit.

```toml
[meta]
name = "Rainbow Wave"
author = "lightplayer"
description = "Smooth rainbow cycling with wave distortion"
tags = ["rainbow", "wave", "classic"]

[resolution]
suggested = [64, 1]

[input.speed]
type = "f32"
label = "Speed"
range = [0.1, 10.0]
default = 1.0

[input.wavelength]
type = "f32"
label = "Wavelength"
range = [0.1, 5.0]
default = 1.0

[input.base_color]
type = "Color"
label = "Base Color"
default = [1.0, 0.5, 0.0]
```

### MVP directory structure

```
rainbow-wave.fx/
  fx.toml          # manifest (required)
  main.glsl        # pixel shader (required)
```

Two files. That's it.

## Notes

- Resolution independence is critical — the system controls resolution for
  performance management (ESP32 has very different perf than a browser GPU).
- The module format should be simple enough that an AI can generate one from
  a text description.
- fw-wasm (device stand-in) is a separate but related workstream.
- WebSerial/espflash for device connection is independent of module design.
- TOML vs JSON: lpfx uses TOML for human-authored manifests. lp-engine
  currently uses JSON (node.json). Reconciliation TBD.

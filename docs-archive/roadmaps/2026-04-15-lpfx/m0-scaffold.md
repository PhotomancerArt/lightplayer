# M0 — Scaffold + First Effect

Create the `lpfx` crate with core types, manifest parsing, and the first
`.fx` module on disk.

## Goal

A loadable effect module exists as a directory with `fx.toml` + `main.glsl`.
The `lpfx` crate can parse the manifest, validate inputs, and produce an
`FxModule` struct. No compilation or rendering yet.

## Deliverables

### `lpfx/lpfx` crate

Core types, all `no_std + alloc` compatible:

```rust
pub struct FxManifest {
    pub meta: FxMeta,
    pub resolution: FxResolution,
    pub inputs: BTreeMap<String, FxInputDef>,
}

pub struct FxMeta {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
}

pub struct FxResolution {
    pub suggested: (u32, u32),
}

pub struct FxInputDef {
    pub input_type: FxInputType,
    pub label: Option<String>,
    pub default: Option<FxValue>,
    pub range: Option<(FxValue, FxValue)>,
    pub presentation: Option<FxPresentation>,
    pub choices: Option<Vec<String>>,
    pub unit: Option<String>,
}

pub enum FxInputType {
    F32,
    I32,
    Bool,
    Color,      // vec3 + color space
    Palette,    // future
}

pub enum FxPresentation {
    Default,    // slider, toggle, etc. based on type
    Choice,     // dropdown for i32
}

pub enum FxValue {
    F32(f32),
    I32(i32),
    Bool(bool),
    Vec3([f32; 3]),
}

pub struct FxModule {
    pub manifest: FxManifest,
    pub glsl_source: String,
}
```

### Manifest parsing

`FxModule::load(path)` reads `fx.toml` + `main.glsl` from a directory.
Error if either file is missing. Validates:
- All inputs have a valid type
- Defaults are type-compatible
- Range values are type-compatible
- `presentation = "choice"` requires `choices` array
- `resolution.suggested` is non-zero

Uses `toml` crate for parsing (already in the workspace or add it).

### First effect: `rainbow-noise.fx`

Located at `lpfx/effects/rainbow-noise.fx/`.

**`fx.toml`:**
```toml
[meta]
name = "Rainbow Noise"
description = "Configurable noise visualization with palette cycling"
tags = ["noise", "rainbow", "classic"]

[resolution]
suggested = [64, 1]

[input.speed]
type = "f32"
label = "Speed"
range = [0.1, 5.0]
default = 1.0

[input.zoom]
type = "f32"
label = "Zoom"
range = [0.01, 0.2]
default = 0.05

[input.noise_fn]
type = "i32"
label = "Noise Function"
presentation = "choice"
choices = ["PSRD Noise", "Worley", "FBM"]
default = 0

[input.palette]
type = "i32"
label = "Palette"
presentation = "choice"
choices = ["Heatmap", "Rainbow", "Fire", "Cool", "Warm"]
default = 1

[input.cycle_palettes]
type = "bool"
label = "Cycle Palettes"
default = true

[input.cycle_time_s]
type = "f32"
label = "Cycle Time"
range = [1.0, 30.0]
default = 5.0
unit = "s"
```

**`main.glsl`:** Adapted from `rainbow.glsl` — same palette functions and
noise demos, but reading uniforms for speed, zoom, noise_fn, palette instead
of hardcoding. Entry point is `vec4 render(vec2 fragCoord, vec2 outputSize,
float time)`.

### Tests

- Parse valid manifest, check all fields
- Parse manifest with missing optional fields (author, tags)
- Error on missing `main.glsl`
- Error on invalid input type
- Error on `choice` without `choices`
- Error on range type mismatch
- Load `rainbow-noise.fx` end-to-end

## Dependencies

None — this is the foundation.

## Validation

```bash
cargo test -p lpfx
cargo check -p lpfx
```

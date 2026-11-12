# Phase 4: noise.fx Effect

## Goal
Create the first real effect module on disk at `examples/noise.fx/`.
The manifest and shader should be self-consistent: every input in
`fx.toml` has a matching uniform in `main.glsl`.

## Files

### 4.1 `examples/noise.fx/fx.toml`

```toml
[meta]
name = "Noise"
description = "Animated noise patterns with palette cycling"
author = "lightplayer"
tags = ["noise", "generative", "palette"]

[resolution]
width = 512
height = 512

[input.speed]
type = "f32"
label = "Speed"
default = 1.0
min = 0.0
max = 10.0

[input.zoom]
type = "f32"
label = "Zoom"
default = 3.0
min = 0.1
max = 20.0

[input.noise_fn]
type = "i32"
label = "Noise Function"
default = 0
presentation = "choice"
choices = [
  { value = 0, label = "PSRDnoise" },
  { value = 1, label = "Worley" },
  { value = 2, label = "FBM" },
]

[input.palette]
type = "i32"
label = "Palette"
default = 0
presentation = "choice"
choices = [
  { value = 0, label = "Rainbow" },
  { value = 1, label = "Sunset" },
  { value = 2, label = "Ocean" },
  { value = 3, label = "Fire" },
  { value = 4, label = "Neon" },
]

[input.cycle_palettes]
type = "bool"
label = "Cycle Palettes"
default = false

[input.cycle_time_s]
type = "f32"
label = "Cycle Time"
default = 8.0
min = 1.0
max = 60.0
unit = "s"
```

### 4.2 `examples/noise.fx/main.glsl`

Adapt from `lp-shader/lps-filetests/filetests/debug/rainbow.glsl`.

Entry point: `vec4 render(vec2 fragCoord, vec2 outputSize, float time)`

Uniforms matching the manifest:
```glsl
uniform float speed;
uniform float zoom;
uniform int noise_fn;
uniform int palette;
uniform bool cycle_palettes;
uniform float cycle_time_s;
```

Content from rainbow.glsl:
- 5 palette functions (rainbow, sunset, ocean, fire, neon)
- palette selection logic + cycling with `mix()`
- 3 noise implementations (psrdnoise, worley, fbm)
- noise selection via `noise_fn` uniform
- Final color = `palette_color(noise_value)`

The shader should compile via `lps_frontend::compile`.

### 4.3 GLSL compile test

Add a `#[test]` (can go in `lib.rs` or a `tests/` directory) that:
1. Loads `examples/noise.fx/fx.toml` via `include_str!`
2. Loads `examples/noise.fx/main.glsl` via `include_str!`
3. Calls `FxModule::from_sources(toml, glsl)` — asserts Ok
4. Calls `lps_frontend::compile(glsl)` — asserts Ok
5. Verifies the manifest has 6 inputs

This test ensures the example stays self-consistent as we iterate.
Note: this test needs a `[dev-dependencies]` on `lps-frontend`.

## Validation
- `cargo test -p lpfx` — all tests pass including GLSL compile test.
- `examples/noise.fx/` exists with both files.
- Manifest inputs match shader uniforms 1:1.

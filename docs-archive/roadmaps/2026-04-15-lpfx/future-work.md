# Future Work

Things explicitly deferred from this roadmap. Documented here so we
don't forget them and so the current design leaves room.

## Q32 emulation on GPU

The CPU path uses Q32 fixed-point arithmetic. The GPU path uses native
float. Visual results differ. For accurate device preview in the browser,
we'd need Q32 emulation on the GPU — either:

- WGSL codegen that emulates Q32 ops with integer math (like lpvm does)
- A naga transform pass that rewrites float ops to Q32-equivalent
- Accept the difference and only use GPU for "close enough" preview

This is a significant effort. The current plan is to accept float
differences for preview.

## Multi-pass / stateful effects

The manifest format should eventually support multiple render passes
with persistent textures for state:

```toml
[[pass]]
name = "update"
shader = "update.glsl"
target = "state"

[[pass]]
name = "render"
shader = "render.glsl"
reads = ["state"]

[texture.state]
format = "rgba16"
size = "output"
persistent = true
```

Enables: fluid simulation, fire, cellular automata, particles,
reaction-diffusion. The MVP single-pass `main.glsl` desugars to a
single `[[pass]]` internally, so the runtime architecture should be
designed to extend naturally.

## Palette type

`Palette` as a first-class input type. Under the hood it's a 1D texture.
The manifest declares it, the UI shows a gradient/palette editor, the
shader reads it as `sampler2D`.

Design questions:
- Color stop representation (positions + colors)
- Interpolation mode (linear, step, smooth)
- Color space per palette
- Built-in palette library (rainbow, fire, cool, warm, ocean, ...)
- Palette cycling/blending as a system feature vs. per-effect logic

## System inputs

Live data streams from hardware/OS:

- **AudioFft** — frequency spectrum (see notes.md for options schema)
- **AudioLevel** — peak + RMS
- **AudioBeat** — beat detection pulse
- **Touch** — multi-touch finger positions
- **MIDI** — CC values, note events
- **DMX** — channel values
- **GPIO** — potentiometer / analog input values
- **MQTT** — arbitrary external values

These all map to the unified input model (everything is an input with a
type and a role). The binding system that wires sources to inputs is an
lp-core concern, not an lpfx concern.

## Input binding system

Any input can be bound to any compatible source:
- A literal value (UI slider)
- A hardware input (potentiometer, DMX, MIDI CC)
- An expression (e.g. `audio_level * 2.0 + 0.5`)
- Another effect's output (modulation)

This lives in lp-core's project/node graph layer, not in lpfx itself.
The effect module just declares its inputs; the system does the wiring.

## Uniform struct for inputs

The M1 `input_` prefix convention (`[input.speed]` → `uniform float input_speed`)
is a workaround for the compiler not yet supporting uniform structs. Once
lps-frontend / naga can handle a `uniform Input { float speed; ... }` block,
refactor to:

```glsl
layout(binding = 0) uniform Input {
    float speed;
    float zoom;
    int noise_fn;
    // ...
};
```

`set_input("speed")` would then map to `set_uniform("input.speed")` — the
manifest stays the same, only the GLSL convention changes. This is also
the natural pattern for WGSL / WebGPU (`var<uniform> input: Input;`).

## Output configuration

Configurable output depth and format:

```toml
[output]
depth = "rgba16"
```

MVP hard-codes RGBA16. Future: let effects suggest a depth, system
overrides based on target capabilities.

## lp-engine integration

An `Effect` node kind in `lp-core` that wraps `FxModule` + `FxInstance`:
- Discovers `.fx` directories in the project filesystem
- Config maps manifest inputs to the node's config/state
- File watching triggers module reload → re-instantiate
- Output texture connects to fixture/output nodes

This replaces the current shader + texture node pair for the common
case of "one effect → one output."

## fw-wasm

Device stand-in for in-browser preview. Uses the same lpvm WASM path
as the CPU renderer but runs the full firmware lifecycle (init, render
loop, LED output). Can "follow" a real device to avoid shipping raw LED
values over serial.

## Effect gallery

The effects picker UI in lp-app. Shows animated thumbnails of available
effects using WebGPU for real-time preview of each. Clicking loads the
effect into the main view.

## AI-driven effect editing

The chat pane in lp-app where you ask an agent to modify the GLSL or
adjust inputs. The agent can:
- Generate a new `main.glsl` from a text description
- Modify an existing shader ("make it more blue", "add more movement")
- Adjust input defaults and ranges
- Create new input definitions

The module format is deliberately simple enough for AI generation.

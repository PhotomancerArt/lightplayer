# Phase 09 — `examples/v1/` corpus migration

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phase 07 merged. Phase 08 in flight is fine; the
> example files don't depend on the loader, only on the Visual
> structs being final.
>
> **Parallel with:** Phase 08 (loader). The loader doesn't need
> these files to compile, and these files don't need the loader.
> Phase 10 depends on both.

## Scope of phase

Stand up the canonical example corpus at
`lp-domain/lp-domain/examples/v1/`. Each example is a complete,
loadable artifact whose round-trip is exercised in Phase 10's
integration tests. Eight TOML files plus one sibling `.glsl` file.

These are the files that prove the design works end-to-end:
authors will copy from these when bootstrapping new artifacts, and
schema-aware editors will use them as completion examples. They
need to be **lean and obvious** — keep parameters to the minimum
that demonstrates the relevant feature; no aspirational fields.

Migrate from the current draft corpus at `docs/design/lpfx/`:
the existing files are M2-vintage and use the **old** schema
(`type = "f32"` / `min` / `max` / `unit` / etc.). The new corpus
follows the M3 grammar (`schema_version = 1`, `kind = "..."`,
`range = [...]`, `default = ...`, `bind = { bus = "..." }`,
unified `[shader]`, OKLCH colors).

References:
- [`docs/design/lightplayer/quantity.md` §10](../../design/lightplayer/quantity.md#10-toml-grammar)
  — TOML grammar; the worked example is the source of truth.
- [`docs/design/color.md`](../../design/color.md) — OKLCH default
  authoring space; `space` field on Color values.
- [`docs/design/lpfx/overview.md`](../../design/lpfx/overview.md)
  — Visual taxonomy and `[bindings]` cascade vocabulary.
- [`docs/design/lightplayer/domain.md`](../../design/lightplayer/domain.md)
  — Visual definitions.

**In scope (files to create at `lp-domain/lp-domain/examples/v1/`):**

```
patterns/
├── rainbow.pattern.toml          (inline GLSL)
├── fbm.pattern.toml              (file = "fbm/main.glsl")
├── fbm/main.glsl                 (sibling shader source)
└── fluid.pattern.toml            (builtin = "fluid"; uses Kind::AudioLevel)
effects/
├── tint.effect.toml              (input = bus video/in/0; Color param)
└── kaleidoscope.effect.toml      (input = bus video/in/0; integer/angle params)
transitions/
├── crossfade.transition.toml     (no input field; softness param)
└── wipe.transition.toml          (no input field; angle / softness params)
stacks/
└── psychedelic.stack.toml        (input = visual; effects = [tint, kaleidoscope])
lives/
└── main.live.toml                (barebones — no [selection]; bindings cascade)
playlists/
└── setlist.playlist.toml         (no per-entry transitions; loop = true)
```

**Files to delete:**

```
docs/design/lpfx/patterns/rainbow.pattern.toml
docs/design/lpfx/patterns/fbm.pattern.toml
docs/design/lpfx/patterns/fluid.pattern.toml
docs/design/lpfx/effects/tint.effect.toml
docs/design/lpfx/effects/kaleidoscope.effect.toml
docs/design/lpfx/transitions/crossfade.transition.toml
docs/design/lpfx/transitions/wipe.transition.toml
docs/design/lpfx/stacks/psychedelic.stack.toml
docs/design/lpfx/lives/main.live.toml
docs/design/lpfx/playlists/setlist.playlist.toml
```

(The corresponding `docs/design/lpfx/{patterns,effects,...}/`
directories should be deleted if empty after the file deletions.
`overview.md` etc. stay where they are.)

**Out of scope:**

- `examples/v1/<kind>/history/` directories — only created when v2
  lands.
- `examples/v1/schemas/` — M4.
- Migration framework (M5) — these files are v1 only.
- Cross-artifact validation of the example corpus — Phase 10's
  integration tests do load + serialize round-trip, not
  resolution.

## Conventions

These are reference TOMLs — they will be cited in design docs
forever. Keep them:

- **Lean.** Minimum params to demonstrate the structural feature.
  No "future" params; add them when an example demands them.
- **Consistent.** Same comment style across all eight files
  (1-2 line comment at the top of each section explaining what
  the artifact is); same param naming where shared
  (`speed`, `intensity`, `color`, etc.).
- **Faithful to grammar.** Every field uses the §10 grammar; no
  shortcut variants until they're documented in `quantity.md`.
- **Round-trip-safe.** Authoring order should match what
  `toml::to_string` will produce after a round-trip
  (`preserve_order` is on per Phase 05). Sub-agent should
  verify by running `cargo test -p lp-domain --test
  round_trip` after Phase 10 lands; if order drifts, adjust the
  authored file's order to match what the serializer emits.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add `[selection]` to `main.live.toml`.
- Do **not** add per-entry `transition` overrides to
  `setlist.playlist.toml`.
- Do **not** use the old M2-vintage grammar (`type = "f32"`,
  `min`/`max`, `unit`, `ui.fader`, etc.). Always:
  `kind = "..."`, `range = [...]`, `default = ...`,
  `present = "..."`.
- Do **not** delete `docs/design/lpfx/overview.md` or other
  non-TOML files in the lpfx design dir — only the moved TOMLs.
- All Color values use `{ space = "oklch", coords = [...] }`
  unless an example exists specifically to demonstrate sRGB
  authoring (none in M3).
- Bus channels follow `<kind>/<dir>/<channel>` (e.g.
  `audio/in/0/level`, `video/in/0`, `time` is the documented
  exception).
- All artifacts have `schema_version = 1` as the **first** field.
- If something blocks, stop and report back.
- Report back: list of changed files (created + deleted), any
  deviations from the templates below.

## Implementation

### `examples/v1/patterns/rainbow.pattern.toml`

```toml
# Cycling rainbow gradient: hue rolls forward at `speed` cycles/sec.
# Single-file ShaderPattern with inline GLSL.

schema_version = 1
title          = "Rainbow"
description    = "Rolling rainbow with HSL hue rotation."
author         = "yona"

[shader]
glsl = """
uniform vec2  outputSize;
uniform float param_time;
uniform float param_speed;
uniform float param_saturation;

vec4 render(vec2 pos) {
  vec2 uv = pos / outputSize;
  float h = fract(uv.x + param_time * param_speed);
  return vec4(lpfn_hsv2rgb(vec3(h, param_saturation, 1.0)), 1.0);
}
"""

[params.time]
kind = "instant"
# `instant` default-binds to "time"; no explicit bind needed.

[params.speed]
kind    = "frequency"
range   = [0.0, 5.0]
step    = 0.1
default = 0.25
label   = "Speed"

[params.saturation]
kind    = "amplitude"
default = 1.0
```

### `examples/v1/patterns/fbm.pattern.toml`

```toml
# Fractal Brownian Motion noise pattern. Shader source is a sibling file
# (`fbm/main.glsl`); this file holds the param surface.

schema_version = 1
title          = "FBM Noise"
description    = "Multi-octave value-noise field."
author         = "yona"

[shader]
file = "fbm/main.glsl"

[params.time]
kind = "instant"

[params.scale]
kind    = "amplitude"
range   = [0.5, 16.0]
default = 4.0
label   = "Scale"

[params.octaves]
kind    = "count"
range   = [1, 8]
default = 4
label   = "Octaves"
```

### `examples/v1/patterns/fbm/main.glsl`

```glsl
uniform vec2  outputSize;
uniform float param_time;
uniform float param_scale;
uniform int   param_octaves;

vec4 render(vec2 pos) {
    vec2  uv  = pos / outputSize;
    float v   = lpfn_fbm(uv * param_scale, param_octaves, param_time);
    return vec4(vec3(v), 1.0);
}
```

(The shader doesn't need to be physically correct — it just has
to be loader-syntactically valid GLSL so Phase 10 can verify the
file is read.)

### `examples/v1/patterns/fluid.pattern.toml`

```toml
# Stam-style fluid sim. Implementation lives in Rust (lpfx::builtins::fluid);
# this artifact carries the parameter surface and the audio binding.

schema_version = 1
title          = "Fluid"
description    = "Stam-style fluid sim with audio-reactive emitters."
author         = "yona"

[shader]
builtin = "fluid"

[params.resolution]
kind    = "count"
range   = [8, 128]
default = 32

[params.viscosity]
kind    = "amplitude"
range   = [0.0, 0.01]
default = 0.0001

[params.fade]
kind    = "amplitude"
range   = [0.5, 1.0]
default = 0.985

[params.solver_hz]
kind    = "frequency"
range   = [10, 60]
default = 30

[params.intensity]
kind    = "audio_level"
default = { low = 0.0, mid = 0.0, high = 0.0 }
# `audio_level` default-binds to "audio/in/0/level"; explicit override
# would go here as `bind = { bus = "..." }`.

[params.emitter_x]
kind    = "amplitude"
default = 0.5

[params.emitter_y]
kind    = "amplitude"
default = 0.5
```

### `examples/v1/effects/tint.effect.toml`

```toml
# Multiply input texture by a target color. Color is OKLCH-authored.

schema_version = 1
title          = "Tint"
description    = "Multiplies the input texture by a target color."
author         = "yona"

[shader]
glsl = """
uniform vec2  outputSize;
uniform vec3  param_color;
uniform float param_amount;
uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    vec4 src = texture(inputColor, pos / outputSize);
    vec3 tinted = mix(src.rgb, src.rgb * param_color, param_amount);
    return vec4(tinted, src.a);
}
"""

[input]
bus = "video/in/0"

[params.color]
kind    = "color"
default = { space = "oklch", coords = [0.7, 0.15, 90] }
label   = "Tint Color"

[params.amount]
kind    = "amplitude"
default = 0.5
label   = "Amount"
```

### `examples/v1/effects/kaleidoscope.effect.toml`

```toml
# Reflects the input around N angular slices.

schema_version = 1
title          = "Kaleidoscope"
description    = "Mirrors the input texture across angular slices."
author         = "yona"

[shader]
glsl = """
uniform vec2  outputSize;
uniform int   param_slices;
uniform float param_rotation;
uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    vec2  uv = pos / outputSize;
    return texture(inputColor, lpfn_kaleidoscope(uv, param_slices, param_rotation));
}
"""

[input]
bus = "video/in/0"

[params.slices]
kind    = "count"
range   = [2, 16]
default = 6
label   = "Slices"

[params.rotation]
kind    = "angle"
default = 0.0
label   = "Rotation"
```

### `examples/v1/transitions/crossfade.transition.toml`

```toml
# Linear cross-fade between inputA and inputB. `progress` is the
# conventional uniform driven by the parent runtime (Live / Playlist).

schema_version = 1
title          = "Crossfade"
description    = "Linear interpolation between two inputs."
author         = "yona"

[shader]
glsl = """
uniform vec2  outputSize;
uniform float progress;
uniform sampler2D inputA;
uniform sampler2D inputB;

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    return mix(texture(inputA, uv), texture(inputB, uv), progress);
}
"""

[params.softness]
kind    = "amplitude"
default = 1.0
```

### `examples/v1/transitions/wipe.transition.toml`

```toml
# Directional wipe. `angle` controls direction, `softness` the band width.

schema_version = 1
title          = "Wipe"
description    = "Directional reveal with a soft edge."
author         = "yona"

[shader]
glsl = """
uniform vec2  outputSize;
uniform float progress;
uniform float param_angle;
uniform float param_softness;
uniform sampler2D inputA;
uniform sampler2D inputB;

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    float t = lpfn_wipe(uv, progress, param_angle, param_softness);
    return mix(texture(inputA, uv), texture(inputB, uv), t);
}
"""

[params.angle]
kind    = "angle"
default = 0.0

[params.softness]
kind    = "amplitude"
range   = [0.0, 1.0]
default = 0.1
```

### `examples/v1/stacks/psychedelic.stack.toml`

```toml
# FBM noise → tint → kaleidoscope.

schema_version = 1
title          = "Psychedelic"
description    = "FBM noise tinted and reflected through a kaleidoscope."
author         = "yona"

[input]
visual = "../patterns/fbm.pattern.toml"
[input.params]
scale = 6.0

[[effects]]
visual = "../effects/tint.effect.toml"

[[effects]]
visual = "../effects/kaleidoscope.effect.toml"
[effects.params]
slices = 8
```

### `examples/v1/lives/main.live.toml`

```toml
# Barebones Live: candidates + default transition + bindings cascade.
# No [selection] block in M3; selection runtime semantics deferred.

schema_version = 1
title          = "Main"
description    = "Audio-reactive fluid with rainbow fallback."

[[candidates]]
visual   = "../patterns/fluid.pattern.toml"
priority = 1.0

[[candidates]]
visual   = "../stacks/psychedelic.stack.toml"
priority = 0.5

[[candidates]]
visual   = "../patterns/rainbow.pattern.toml"
priority = 0.1

[transition]
visual   = "../transitions/crossfade.transition.toml"
duration = 2.0

# Cascade: descendant slot keys → bus bindings. Keys are raw
# relative-NodePropSpec strings in M3 (no parsing yet).
[bindings]
"candidates/0#emitter_x" = { bus = "touch/in/0/x" }
"candidates/0#emitter_y" = { bus = "touch/in/0/y" }
```

### `examples/v1/playlists/setlist.playlist.toml`

```toml
# Sequenced setlist with a single default transition. `loop = true`
# restarts from the first entry after the last.

schema_version = 1
title          = "Setlist"
description    = "Three-track choreographed sequence."

[[entries]]
visual   = "../patterns/fluid.pattern.toml"
duration = 60.0

[[entries]]
visual   = "../patterns/fbm.pattern.toml"
duration = 90.0

[[entries]]
visual   = "../stacks/psychedelic.stack.toml"
duration = 75.0

[[entries]]
visual   = "../patterns/rainbow.pattern.toml"
# no duration → wait for cue

[transition]
visual   = "../transitions/crossfade.transition.toml"
duration = 1.5

[behavior]
loop = true

[bindings]
"entries/1#scale"            = { bus = "audio/in/0/level" }
```

## Validate

```bash
# After Phase 10 lands, the round-trip suite is the real validation.
# This phase's local validation: the files must exist and the
# loader (Phase 08) must parse them without error.
cargo test -p lp-domain --test round_trip || true
```

(If the round-trip suite hasn't landed yet, write a one-shot
ad-hoc test under `cargo run --example load_corpus` or similar
to confirm each file loads. Phase 10 will replace it.)

```bash
# Confirm the old draft corpus is gone.
ls docs/design/lpfx/patterns/    # should not exist OR be empty of .toml
ls docs/design/lpfx/effects/     # ditto
# (and the other four kinds)
```

## Definition of done

- All eight TOML files + one `.glsl` file exist at the documented
  paths under `lp-domain/lp-domain/examples/v1/`.
- All ten files at `docs/design/lpfx/{patterns,effects,
  transitions,stacks,lives,playlists}/*.toml` are deleted.
- Each example uses the M3 grammar (no M2-vintage `type` /
  `min`/`max` / `unit` fields).
- Color defaults use `{ space = "oklch", coords = [...] }`.
- `fluid.pattern.toml` uses `kind = "audio_level"` for
  `intensity`.
- `main.live.toml` has no `[selection]` block.
- `setlist.playlist.toml` has no per-entry `transition` overrides.
- All channel names follow `<kind>/<dir>/<channel>` (e.g.
  `audio/in/0/level`, `video/in/0`, `touch/in/0/x`).
- No commit.

Report back with: list of created files, list of deleted files,
any deviations from the templates above.

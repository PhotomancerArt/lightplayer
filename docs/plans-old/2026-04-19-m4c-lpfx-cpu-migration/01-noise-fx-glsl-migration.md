# Phase 1 — `noise.fx` GLSL signature migration

`[sub-agent: yes, parallel: 2]`

## Scope of phase

Migrate `examples/noise.fx/main.glsl` from the legacy 3-arg render
signature to the new `render(vec2 pos)` + uniforms contract enforced
by `LpsEngine::compile_px`. Single GLSL file edit. Mirrors what M4a
did for every engine shader (e.g. `examples/basic/src/rainbow.shader/main.glsl`).

Required end-state of `examples/noise.fx/main.glsl`:

- Adds two new uniform declarations: `outputSize` (`vec2`) and `time`
  (`float`). Use `layout(binding = 0) uniform …` to match the existing
  6 `input_*` uniforms in this file.
- The `render` function changes from
  `vec4 render(vec2 fragCoord, vec2 outputSize, float time) { … }`
  to `vec4 render(vec2 pos) { … }`.
- All references to the old `fragCoord` parameter inside the body
  become `pos`.
- All references to the old `outputSize` and `time` parameters inside
  the body now bind to the newly-declared uniforms (i.e. just delete
  the parameters; the body keeps using the names — they now resolve
  to the file-level uniforms instead of locals).
- No semantic change. No body logic edits beyond rename
  `fragCoord` → `pos`.

## Out of scope

- Anything in `lpfx/lpfx/src/`, `lpfx/lpfx-cpu/src/`, `lpfx/*/Cargo.toml`.
  Those are phases 2 and 3.
- Any other shader in `examples/`. Only `examples/noise.fx/main.glsl`.
- Editing `examples/noise.fx/fx.toml` (the `[input.X]` section is
  unaffected — the manifest inputs map to `input_X` uniforms which
  already exist with the right shape).
- Running any Rust build or test. Phase 1 only edits GLSL; phase 3
  exercises it. `lps_frontend::compile(NOISE_FX_GLSL)` is exercised
  by `lpfx/lpfx/src/lib.rs::tests::noise_fx_compiles_in_lps_frontend`,
  which the phase 2 sub-agent will run as part of `cargo check -p lpfx`
  / `cargo test -p lpfx` — phase 1 doesn't need to validate Rust.

## Code organization reminders

- One concept per file. The GLSL file already groups palettes,
  noise demos, dispatch, and `render` in that order. Keep it.
- Place the two new uniform declarations next to the existing
  `input_*` uniforms at the top of the file (they're file-level
  uniforms, same kind).
- No `TODO` comments. The change is complete in this phase.

## Sub-agent reminders

- Do **not** commit. The whole plan commits as one unit at phase 4.
- Do **not** expand scope. Only `examples/noise.fx/main.glsl` is in
  bounds. Do **not** touch `examples/noise.fx/fx.toml`, any other
  example, or any Rust code.
- Do **not** add any new `input_*` uniform. The new uniforms are
  exactly two: `outputSize` (`vec2`) and `time` (`float`). They are
  *not* prefixed with `input_` because they aren't manifest inputs —
  they're engine-driven (mirror what `lp-engine`'s shaders do, e.g.
  `examples/basic/src/rainbow.shader/main.glsl`).
- Do **not** rename, reorder, or rewrite any helper function
  (`paletteRainbow`, `prsd_demo`, `pick_noise`, etc.). Only edit
  `render`.
- Do **not** change logic inside `render`'s body beyond the
  `fragCoord` → `pos` rename. The post-edit body must be
  byte-for-byte equivalent to the pre-edit body in the parts that
  reference `outputSize`, `time`, or `pos`.
- If anything is ambiguous or blocked, **stop and report**.
- Report back: the diff of `examples/noise.fx/main.glsl`.

## Implementation details

### Reference: how M4a-migrated shaders look

`examples/basic/src/rainbow.shader/main.glsl` is the canonical
post-M4a shape — engine-driven uniforms at the top, `render(vec2 pos)`
at the bottom. Match that style.

### File — `examples/noise.fx/main.glsl`

Current (truncated):

```glsl
layout(binding = 0) uniform float input_speed;
layout(binding = 0) uniform float input_zoom;
layout(binding = 0) uniform int input_noise_fn;
layout(binding = 0) uniform int input_palette;
layout(binding = 0) uniform bool input_cycle_palettes;
layout(binding = 0) uniform float input_cycle_time_s;

… helpers …

vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
    float t = time * input_speed;
    float s = 0.05 * input_zoom;
    vec2 center = outputSize * 0.5;
    vec2 dir = fragCoord - center;
    vec2 scaledCoord = center + dir * s;

    vec2 tv = pick_noise(scaledCoord, t);

    vec3 col;
    if (input_cycle_palettes) {
        float period = max(input_cycle_time_s, 0.001);
        float u = mod(t / period, 1.0);
        float a = floor(u * 5.0);
        float b = mod(a + 1.0, 5.0);
        float w = fract(u * 5.0);
        w = smoothstep(0.0, 1.0, w);
        vec3 c0 = applyPalette(tv.x, int(a));
        vec3 c1 = applyPalette(tv.x, int(b));
        col = mix(c0, c1, w);
    } else {
        col = applyPalette(tv.x, input_palette);
    }

    return vec4(col * tv.y, 1.0);
}
```

Target:

```glsl
layout(binding = 0) uniform float input_speed;
layout(binding = 0) uniform float input_zoom;
layout(binding = 0) uniform int input_noise_fn;
layout(binding = 0) uniform int input_palette;
layout(binding = 0) uniform bool input_cycle_palettes;
layout(binding = 0) uniform float input_cycle_time_s;
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 0) uniform float time;

… helpers (unchanged) …

vec4 render(vec2 pos) {
    float t = time * input_speed;
    float s = 0.05 * input_zoom;
    vec2 center = outputSize * 0.5;
    vec2 dir = pos - center;
    vec2 scaledCoord = center + dir * s;

    vec2 tv = pick_noise(scaledCoord, t);

    vec3 col;
    if (input_cycle_palettes) {
        float period = max(input_cycle_time_s, 0.001);
        float u = mod(t / period, 1.0);
        float a = floor(u * 5.0);
        float b = mod(a + 1.0, 5.0);
        float w = fract(u * 5.0);
        w = smoothstep(0.0, 1.0, w);
        vec3 c0 = applyPalette(tv.x, int(a));
        vec3 c1 = applyPalette(tv.x, int(b));
        col = mix(c0, c1, w);
    } else {
        col = applyPalette(tv.x, input_palette);
    }

    return vec4(col * tv.y, 1.0);
}
```

Concrete diff:

1. After the existing 6 `input_*` uniform declarations, append two
   lines:

   ```glsl
   layout(binding = 0) uniform vec2 outputSize;
   layout(binding = 0) uniform float time;
   ```

2. In the `render` function header, replace:

   ```glsl
   vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
   ```

   with:

   ```glsl
   vec4 render(vec2 pos) {
   ```

3. In the `render` body, replace `fragCoord` with `pos` (one
   occurrence: the `vec2 dir = fragCoord - center;` line). All other
   references to `outputSize` and `time` inside the body stay
   unchanged — they now resolve to the file-level uniforms instead
   of the (now-removed) function parameters.

That's the entire change. The body's reference count to `time`,
`outputSize`, and (now) `pos` is identical to before.

## Validate

No build commands. The shader is exercised by phase 2 + 3:

- Phase 2 will run `cargo check -p lpfx` / `cargo test -p lpfx`,
  which includes `noise_fx_compiles_in_lps_frontend` (calls
  `lps_frontend::compile(NOISE_FX_GLSL)`). That confirms the GLSL
  parses against `lps-frontend`'s naga front end.
- Phase 3's `noise_fx_renders_nonblack` and `noise_fx_default_inputs`
  tests then exercise the full compile-and-render pipeline through
  `LpsEngine::compile_px`, which runs `validate_render_sig` (rejects
  the legacy 3-arg form) and `synthesise_render_texture`. Either test
  passing is end-to-end proof the migration is correct.

When reporting back, paste the final diff so the parent agent can
sanity-check the rename was the only body change.

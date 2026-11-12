# LPFX M0 — Scaffold + First Effect — Notes

## Scope

Create the `lpfx/lpfx` crate with core types, TOML manifest parsing,
validation, and the first `.fx` effect module on disk (`rainbow-noise.fx`).
No compilation or rendering — that's M1/M2.

Roadmap context: `docs/roadmaps/2026-04-15-lpfx/`

## Current State

- **No `lpfx/` directory** exists yet in the workspace.
- **`toml` crate** is not in the workspace. Needs to be added.
- **`serde`** is at 1.0.228 workspace-wide, `default-features = false,
  features = ["alloc"]`. Crates add `derive` locally.
- **Existing config pattern**: `lp-model` types use `#[derive(Serialize,
  Deserialize)]`, loaded via `serde_json` / `serde-json-core`.
- **LpFs** lives in `lp-core/lp-shared` — lpfx should not depend on lp-core.
- **`rainbow.glsl`** exists in `lp-shader/lps-filetests/filetests/debug/`
  with 5 palette functions, 3 noise demos (psrdnoise, worley, fbm), and
  palette cycling logic. This is the basis for `rainbow-noise.fx/main.glsl`.

## Questions

### Q1: no_std for lpfx core types?

**Answer**: `no_std + alloc` throughout, like `lp-model`. Firmware uses
these types. TOML parsing is also `no_std` — the `toml` crate v0.9+
(July 2025) supports `no_std`. No feature gate needed; the entire lpfx
crate works on bare metal.

### Q2: Filesystem loading approach?

**Answer**: `FxModule::from_sources(toml: &str, glsl: &str)`. Takes raw
strings, caller reads from whatever filesystem. Tests use `include_str!`
or literals. lpfx has no dependency on lp-core/LpFs.

### Q3: Where does the first effect live?

**Answer**: `examples/noise.fx/` at the workspace root. Short name —
it's not always a rainbow.

### Q4: How to handle the TOML → serde mapping for inputs?

**Answer**: Raw deserialization struct (`RawManifest` / `RawInputDef`)
with string and `toml::Value` fields, then validate and convert to typed
`FxManifest`. Separates parsing from validation, gives good error
messages. `#[serde(rename = "type")]` for the keyword conflict.

### Q5: Workspace integration — where in Cargo.toml?

**Answer**: Add `"lpfx/lpfx"` to both `members` and `default-members`.
Add `toml = { version = "0.9", default-features = false }` to
`[workspace.dependencies]`.

### Q6: What about the main.glsl for noise.fx?

**Answer**: Write the full shader. Adapt `rainbow.glsl` — replace
hardcoded values with uniforms (`speed`, `zoom`, `noise_fn`, `palette`,
`cycle_palettes`, `cycle_time_s`). Keep palette functions and noise
demos. Branch on `noise_fn` for noise selection. Entry point is
`vec4 render(vec2 fragCoord, vec2 outputSize, float time)`.

Add a test that the GLSL parses via `lps_frontend::compile` to catch
syntax errors, even though M0 doesn't render. The fx.toml and main.glsl
should be self-consistent — every input declared in the manifest has a
matching uniform in the shader.

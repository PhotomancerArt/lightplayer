# Phase 4: Convert Live Examples and Comments

## Metadata

- **sub-agent:** yes
- **model:** composer-2
- **parallel:** 3

## Scope of Phase

Convert live example project config files from `node.json` to `node.toml`, and
update live comments that refer to those example config paths.

In scope:

- Convert every `examples/**/node.json` file to `node.toml`.
- Remove the old example `node.json` files after conversion.
- Preserve each example's config semantics.
- Update live code comments that point to example `node.json` paths.

Out of scope:

- Do not update archived plan files or historical roadmap/design docs.
- Do not modify runtime loader code.
- Do not modify builders/templates/tests outside example references.
- Do not change GLSL files or example project structure beyond config file
  format.

## Code Organization Reminders

- Keep conversions mechanical and scoped.
- Preserve existing example directory layout.
- Keep related edits grouped by example/project.
- Any temporary code must have a `TODO` comment with a clear follow-up.

## Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to hide problems.
- Do not disable, skip, or weaken existing tests.
- If a JSON file does not match the current config structs, stop and report
  instead of guessing.
- Report back: files changed, validation run, validation result, and deviations.

## Implementation Details

Read the shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-design.md`

Known live example files:

```text
examples/basic/src/main.texture/node.json
examples/basic/src/rainbow.shader/node.json
examples/basic/src/strip.output/node.json
examples/basic/src/fixture.fixture/node.json
examples/basic2/src/main.texture/node.json
examples/basic2/src/rainbow.shader/node.json
examples/basic2/src/strip.output/node.json
examples/basic2/src/fixture.fixture/node.json
examples/fast/src/main.texture/node.json
examples/fast/src/simple.shader/node.json
examples/fast/src/strip.output/node.json
examples/fast/src/fixture.fixture/node.json
examples/perf/baseline/src/main.texture/node.json
examples/perf/baseline/src/rainbow.shader/node.json
examples/perf/baseline/src/strip.output/node.json
examples/perf/baseline/src/fixture.fixture/node.json
examples/perf/fastmath/src/main.texture/node.json
examples/perf/fastmath/src/rainbow.shader/node.json
examples/perf/fastmath/src/strip.output/node.json
examples/perf/fastmath/src/fixture.fixture/node.json
```

Convert each JSON file to TOML matching the same serde shape. Examples:

Texture:

```toml
width = 16
height = 16
```

Shader:

```toml
glsl_path = "main.glsl"
texture_spec = "/src/main.texture"
render_order = 0

[glsl_opts]
add_sub = "wrapping"
mul = "wrapping"
div = "reciprocal"
```

Output enum shape should match serde's TOML representation for
`OutputConfig::GpioStrip`. Prefer generating TOML with the current Rust config
structs if you are unsure about enum encoding.

Fixture mappings are nested enum-heavy data. Prefer using a small temporary
conversion script or Rust/test helper that deserializes each existing JSON config
into the appropriate legacy config struct and serializes it as TOML. If you use
a script, keep it outside the final diff or delete it before reporting back.

Live comments to update include:

- `lp-fw/fw-esp32/src/tests/fluid_demo/ring_geometry.rs`

Use search to find any remaining non-historical live references to
`examples/.../node.json`.

Do not edit `docs/plans-old/**`. Historical docs can keep historical file
names.

## Validate

Run from the repository root:

```bash
rg "examples/.*/node\\.json|node\\.json" examples lp-fw/fw-esp32/src/tests/fluid_demo
```

The command should return no live example/config comment references after this
phase.

# Phase 4: Examples And Templates

## Scope Of Phase

Update user-facing examples and project generators to exercise visual shader
consumed inputs.

In scope:

- Update examples using regular visual shaders to `kind = "shader/visual"`.
- Add `[consumed.time]` to examples whose GLSL uses `time`.
- Remove stale `texture_loc` fields where examples no longer use that model.
- Update CLI/server templates and project builder output.
- Validate representative examples load.

Out of scope:

- Redesigning old examples unrelated to shader time.
- Texture inputs.
- Real UI controls beyond existing debug UI.

## Code Organization Reminders

- Keep example TOML minimal and idiomatic.
- Prefer relying on default time binding when possible; only author
  `[bindings.time]` when deliberately overriding the convention.
- Preserve example-specific GLSL unless it must change to compile.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant paths:

- `examples/basic`
- `examples/basic2`
- `examples/fast`
- `examples/rocaille`
- `examples/perf/baseline`
- `examples/perf/fastmath`
- `lp-cli/src/commands/create/project.rs`
- `lp-app/lpa-server/src/template.rs`
- `lp-core/lpc-shared/src/project/builder.rs`

Expected TOML for visual shader time:

```toml
kind = "shader/visual"
glsl_path = "shader.glsl"

[consumed.time]
kind = "value"
value = "f32"
```

Expected behavior:

- Examples with a clock node and `consumed.time` should animate from
  `bus#time.seconds` without explicit `bindings.time`.
- If an example does not include a clock, either add inline clock or do not
  declare `consumed.time`.

Validation targets:

- Basic example loads.
- Fluid example still loads.
- At least one older visual shader example loads after conversion.

## Validate

```bash
cargo fmt
cargo test -p lpc-engine project_loader
cargo check -p lp-cli
```


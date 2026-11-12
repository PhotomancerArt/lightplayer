# Phase 1: Example Files

## Scope Of Phase

Create the new `examples/fluid` source project.

In scope:

- Add `project.toml`.
- Add `compute.toml` and `compute.glsl`.
- Add `fluid.toml`.
- Add `fixture.toml` and `output.toml`, adapted from `examples/basic`.
- Keep the example small enough to run on ESP32-C6.

Out of scope:

- Changing `examples/basic`.
- Profiling.
- UI changes.

## Code Organization Reminders

- Keep one artifact per file.
- Do not add a texture artifact unless the runtime requires it.
- Keep GLSL comments short and useful.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Create:

```text
examples/fluid/project.toml
examples/fluid/compute.toml
examples/fluid/compute.glsl
examples/fluid/fluid.toml
examples/fluid/fixture.toml
examples/fluid/output.toml
```

Use bus-first bindings:

- compute `emitters` -> `bus#fluid.emitters`
- fluid `emitters` <- `bus#fluid.emitters`
- fluid `output` -> `bus#visual.out`
- fixture `input` <- `bus#visual.out`
- fixture `output` -> `bus#control.out`
- output `input` <- `bus#control.out`

Use the existing basic ring fixture geometry for the first version.

`compute.toml` should define:

```toml
kind = "shader/compute"
glsl_path = "compute.glsl"

[produced.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
```

## Validate

```bash
cargo test -p lpc-model fluid -- --nocapture
cargo test -p lpc-engine fluid -- --nocapture
```

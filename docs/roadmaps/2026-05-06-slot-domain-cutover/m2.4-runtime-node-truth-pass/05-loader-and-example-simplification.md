# Phase 5: Loader And Example Simplification

## Scope Of Phase

Update project loading and canonical examples so the MVP runtime flow is shader -> fixture -> output.

In scope:

- Make `CoreProjectLoader` resolve fixture `bindings.input` to shader output directly, including bus-mediated bindings.
- Remove loader dependency on texture nodes for canonical shader/fixture flow.
- Stop finding a shader by first finding a texture.
- Keep `TextureDef` / `TextureNode` support only if it remains easy and isolated.
- Update `examples/basic` last.
- Update source/loader tests.

Out of scope:

- Deleting every texture type from the repo.
- Texture resource design.
- Wire/view rebuild.

## Code Organization Reminders

- Keep binding-resolution helpers in `project_loader.rs` unless they become large enough to deserve their own file.
- Prefer clear helper names around authored binding direction: source vs target.
- Do not keep old texture lookup helpers if no longer used.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `examples/basic/project.toml`
- `examples/basic/shader.toml`
- `examples/basic/fixture.toml`
- `examples/basic/output.toml`
- `lp-core/lpc-source/tests/basic_example_parse.rs`

Loader direction:

- `ShaderDef.bindings.output.target` may be a bus or direct node slot.
- `FixtureDef.bindings.input.source` should resolve to that shader output, either:
  - direct: `..shader#output`
  - bus: shader targets `bus#visual.out`, fixture sources `bus#visual.out`
- `FixtureDef.output_loc` still resolves output sink.
- Fixture constructor should receive whatever render target dimensions/config it needs without runtime texture-node dependency.

Canonical `examples/basic` should no longer require `texture.toml`.

Expected project shape:

```toml
kind = \"project\"
name = \"basic\"

[nodes.output]
artifact = \"./output.toml\"

[nodes.shader]
artifact = \"./shader.toml\"

[nodes.fixture]
artifact = \"./fixture.toml\"
```

Expected bindings:

```toml
# shader.toml
[bindings.output]
target = \"bus#visual.out\"

# fixture.toml
[bindings.input]
source = \"bus#visual.out\"
```

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine project_runtime::project_loader
cargo test -p lpc-source --test basic_example_parse
```

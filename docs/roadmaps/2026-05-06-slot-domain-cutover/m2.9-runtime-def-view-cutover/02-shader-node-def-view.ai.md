# Phase 2: Shader Node Def View

## Scope Of Phase

Convert `ShaderNode` so it does not store a cloned `ShaderDef` for runtime
config. Shader config read during runtime should go through `ShaderDefView`.

In scope:

- Add `ShaderDefView` cache to `ShaderNode`.
- Remove `config: ShaderDef` from `ShaderNode`.
- Keep loader-side GLSL source loading from authored `ShaderDef.glsl_path`.
- Read GLSL compile options during `tick()` through the resolver-backed view.
- Store compact runtime compile options/model options on the node for render.

Out of scope:

- Dynamic shader params.
- Runtime reload of `glsl_path` / GLSL source file.
- Giving `RenderContext` resolver access.

## Code Organization Reminders

- Keep helper mapping functions near shader-node compile logic.
- Avoid a broad abstraction until more nodes use it.
- Keep tests at the bottom of the file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-model/src/nodes/shader/glsl_opts.rs`
- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`

Expected changes:

- Change `ShaderNode::new(node_id, config, glsl_source)` to
  `ShaderNode::new(node_id, glsl_source)`.
- Add `def_view: Option<ShaderDefView>`.
- Add a small `def_view(&mut self, ctx: &TickContext<'_>)` helper, mirroring
  `TextureNode`.
- In `tick()`, resolve the compile-option fields through accessors and update
  cached `GlslOpts` or directly cached `ShaderCompileOptions`.
- `ensure_compiled` should use the cached options instead of `self.config`.
- Update loader and tests for the thinner constructor.

Constraints:

- If `GlslOpts` cannot yet be read as a whole aggregate through the current
  accessor helper, read its scalar/enum leaves via generated field accessors or
  add minimal view coverage needed for the `GlslOpts` fields.
- Do not add resolver access to `RenderContext` in this phase.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine shader
cargo test -p lpc-engine project_runtime::project_loader
```

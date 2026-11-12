# Phase 4: Runtime Source Updates And Live Reload

- **parallel:** -
- **sub-agent:** main

## Scope Of Phase

Teach shader runtime nodes to compile from versioned source updates and restore
live reload for both node definitions and shader source through artifact/source
revision invalidation.

In scope:

- Replace raw `glsl_source: String` ownership in shader runtime nodes.
- Expose source resolution from both `TickContext` and `RenderContext`.
- Recompile shaders when source version changes.
- Keep unchanged source checks cheap.
- Implement `Engine::handle_fs_changes` enough to bump affected source
  revisions, reload changed node definitions, and reconcile/recreate affected
  runtime nodes.
- Add tests that modify shader source and node definition TOML and observe the
  expected runtime updates.

Out of scope:

- Dependency/import graphs inside shader source.
- Cross-language shader compilation.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Keep tests at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

### Context APIs

Files:

- `lp-core/lpc-engine/src/node/contexts.rs`
- source resolver files from Phase 2.

Add convenience methods with equivalent semantics on both relevant contexts:

```rust
ctx.resolve_shader_source(source, last_seen_version)
```

For `TickContext`, the source service may be reached through the active resolver
or a context field.

For `RenderContext`, add a shared source service field or route through the
engine render materialization service. Visual shader compilation currently only
receives `RenderContext`, so do not hide source resolution only on `TickContext`.

### Visual shader node

File:

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`

Current state:

- owns `glsl_source: String`,
- compiles once and keeps the compiled shader,
- stores `compilation_error` until cleared by node recreation.

New state:

```rust
source: ShaderSource or ResolvedShaderSourceHandle,
last_source_version: Option<SourceVersion>,
shader: Option<Box<dyn LpShader>>,
compilation_error: Option<String>,
```

New behavior:

- Before compiling/rendering, ask for source update.
- If unchanged and compiled shader exists, reuse it.
- If changed, clear `shader` and `compilation_error`.
- Compile newly materialized source text.
- Store the new source version after compile attempt has observed the new text.
- If compile fails, keep the failed version noted so repeated renders do not
  recompile the exact same failing source every frame unless the source changes.

### Compute shader node

File:

- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs`

Apply the same versioned source behavior.

Compute-specific requirement:

- The effective source should include generated compute header plus authored
  source.
- Source version should include the shader source content revision and any
  header-affecting revision. At minimum include the node artifact/content frame
  or slot shape registry revision used to generate the header.
- Header generation should happen close to compile/materialization, not during
  project load, so source/header changes are represented in one compile key.

### Filesystem change handling

File:

- `lp-core/lpc-engine/src/engine/engine.rs`

Current:

```rust
pub fn handle_fs_changes(&mut self, _changes: &[FsChange]) -> Result<(), EngineError> {
    Ok(())
}
```

New behavior:

- For each changed path, find registered node-definition and shader-source
  artifact locations that map to that path.
- For shader sources on create/modify, bump content revision and invalidate
  cached materialized text.
- For shader sources on delete, bump content revision and record the
  missing-source state so the next source materialization reports a useful
  error.
- For node-definition artifacts on create/modify, re-read and parse the
  `NodeDef`, update the artifact payload/revision, rebuild affected slot
  shapes/bindings as needed, and reconcile or recreate the corresponding
  runtime node.
- For node-definition artifacts on delete, move the artifact into a load-error
  state and surface the error through project state instead of panicking.
- For inline node definitions and inline shader sources, treat the owning TOML
  artifact revision as part of the effective child/source version.
- Do not call shader-specific reload hooks directly.

If the source resolver cannot efficiently map path to source ID, add a reverse
index when sources are registered. If node-definition artifacts cannot be mapped
from path to `NodeId`, extend the artifact/node index rather than inventing fake
paths for inline nodes.

### Node definition reconciliation

Node definition reload may require more than source recompilation.

Required behavior:

- If a node def changes but keeps the same node kind and compatible runtime
  structure, update/recreate the runtime node using the fresh def.
- If a node def changes kind or shape in a way that invalidates the runtime
  node, destroy and recreate the runtime node through normal engine ownership.
- Re-register changed runtime state slot shapes and consumed/produced slot
  metadata as needed.
- Rebuild bindings affected by changed node defs or changed inline project
  definitions.
- Preserve stable tree identity for the child node where practical so external
  references remain coherent.
- Surface reload errors in artifact/project state and leave the previous working
  runtime in place if that is the safer behavior for a failed reload.

### Tests

Add tests for:

- unchanged visual shader source does not recompile across frames/renders,
- modifying a file-backed GLSL source bumps version and recompiles,
- modifying inline GLSL through owner artifact revision bumps effective source
  version,
- modifying a split-file shader node def reloads the node def and affects
  runtime behavior,
- modifying an inline shader node def inside `project.toml` reloads the inline
  node def and affects runtime behavior,
- changing a node def's shader source path causes the runtime node to observe
  the new source identity/version,
- node def parse errors are surfaced without panicking,
- compile errors are retried only after source version changes,
- compute shader source updates rebuild the compute descriptor/header as needed,
- deleting a source file reports an error without panicking.

Use fake graphics backends that count compile calls where possible.

## Validate

Run:

```bash
cargo test -p lpc-engine shader_node --lib
cargo test -p lpc-engine compute_shader_node --lib
cargo test -p lpc-engine project_loader --lib
cargo check -p lpc-engine
```

Because this phase touches the shader pipeline, also run:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

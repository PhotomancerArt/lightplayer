# Phase 4: Minimal Loader Interpretation

## Scope Of Phase

Teach the project loader enough about semantic authored bindings to prove they
can drive runtime binding intent, while leaving the full runtime node flow
refactor to M2.4.

In scope:

- Read `BindingDefs` from loaded node defs.
- Convert supported authored endpoints into runtime binding drafts or an
  inspectable intermediate intent.
- Preserve current runtime behavior where necessary until M2.4.
- Add local tests proving loader sees the new bindings.

Out of scope:

- Refactoring `ShaderNode`, `TextureNode`, or `FixtureNode` runtime semantics.
- Full bus resolution.
- Runtime slot root exposure.
- Output sink redesign.

## Code Organization Reminders

- Keep transitional loader code small and clearly named.
- Prefer helper functions in `project_loader.rs` only if they are loader-local.
- If conversion grows domain meaning, move it to an appropriate engine binding
  module instead of burying it.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/binding/binding_entry.rs`
- `lp-core/lpc-engine/src/binding/binding_registry.rs`
- `lp-core/lpc-source/src/node/*`

Current loader behavior is imperative:

- shader finds a texture through `ShaderDef.texture_loc`
- fixture finds texture/output through `FixtureDef.texture_loc` and
  `FixtureDef.output_loc`
- fixture finds shader through `find_shader_for_texture`

M2.3 should not fully replace this flow if doing so requires M2.4 runtime node
changes. Instead:

- add loader helpers that can inspect/resolve `BindingDefs`
- support parsing:
  - `target = "bus#visual.out"`
  - `source = "bus#visual.out"`
  - `source = "..shader#output"`
- where runtime `BindingRegistry` can accept an equivalent draft safely, register
  it
- where the runtime cannot yet consume it, keep an explicit transitional path
  with a TODO pointing to M2.4

Tests to add/update:

- project loader test sees shader output bus target.
- project loader test sees texture input bus source.
- direct node-slot endpoint resolves relative to the current node.
- existing runtime load tests remain green, possibly through transitional
  compatibility only.

## Validate

Run:

```bash
cargo fmt --package lpc-engine
cargo test -p lpc-engine project_loader
cargo check -p lpc-engine
```

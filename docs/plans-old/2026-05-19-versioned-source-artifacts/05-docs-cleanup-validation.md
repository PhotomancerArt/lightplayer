# Phase 5: Docs, Cleanup, And Final Validation

- **parallel:** -
- **sub-agent:** main

## Scope Of Phase

Finish the plan by removing temporary bridges, documenting the new authored
shapes and runtime source boundary, updating all examples, and running final
validation.

In scope:

- Remove temporary bridge TODOs that are no longer needed.
- Remove old authored-field support for `artifact` and `glsl_path`.
- Update docs for node invocation and shader source.
- Update every example to the new authored model.
- Run final validation commands and fix failures.

Out of scope:

- Adding backwards-compatible reads for legacy `artifact` or `glsl_path`.
- Adding real WGSL support.

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

### Cleanup

Search for and resolve temporary markers and old authored fields introduced or
left behind by earlier phases:

```bash
rg -n "TODO|temporary|compatibility bridge|glsl_path|artifact = " lp-core/lpc-model/src lp-core/lpc-engine/src docs examples
```

Old authored fields should not remain accepted. Keep mentions only where they
describe the current-state migration or explicit rejection tests.

### Docs

Create or update:

- `docs/design/source-artifacts.md`
- any existing docs that describe project TOML, shader nodes, or examples.

Document:

- `def` as the child node definition namespace inside `nodes.<name>`,
- `def.path` as the canonical authored external node definition reference,
- `[source] path = ...`,
- `[source] glsl = ...`,
- one-file project examples,
- the migration from old `artifact`/`glsl_path` fields to the new authored
  model, explicitly noting that backwards-compatible reads are not retained,
- runtime rule that nodes resolve versioned source through context APIs and do
  not watch files,
- reload behavior for split-file node defs, inline node defs, split-file shader
  sources, and inline shader sources.

Mention that `wgsl` is intentionally reserved/future-facing but not implemented
unless it was actually added.

### Examples

Update all examples under `examples/` to the new authored model:

- replace `[nodes.x] artifact = ...` with `[nodes.x] def = { path = ... }`,
- replace `glsl_path = ...` with `[source] path = ...`,
- convert simple examples to one-file inline node/source form where that reduces
  clutter,
- keep at least one split-file example to exercise `def.path` and
  `source.path`.

### Final review checklist

Verify:

- Nodes do not own filesystem roots.
- Nodes do not receive `FsChange`.
- Split-file node definition reload works.
- Inline node definition reload through the owning project TOML works.
- Shader nodes compare opaque source versions.
- Unchanged source checks do not read file bytes.
- File-backed and inline GLSL source go through the same node-facing API.
- Old `artifact` and `glsl_path` authored shapes are not accepted silently.
- No compiler path is gated behind `std`.
- No shader pipeline dependency breaks `no_std + alloc`.

## Validate

Run targeted validation:

```bash
cargo test -p lpc-model
cargo test -p lpc-slot-macros
cargo test -p lpc-engine project_loader --lib
cargo test -p lpc-engine shader_node --lib
cargo test -p lpc-engine compute_shader_node --lib
cargo check -p lpc-engine
```

Run shader-pipeline validation from `AGENTS.md`:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

Before pushing, if this plan is being taken through CI readiness, run:

```bash
just check
just build-ci
just test
```

Do not run `cargo build --workspace` or `cargo test --workspace`.

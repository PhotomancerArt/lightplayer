# Phase 2: Switch Legacy Runtime Loader to node.toml

## Metadata

- **sub-agent:** yes
- **model:** composer-2
- **parallel:** -

## Scope of Phase

Switch the legacy runtime loading and hot-reload path from direct `node.json`
parsing to the source-owned `node.toml` loader added in Phase 1.

In scope:

- Update `lpc-engine` legacy loader code to call the new `lpc-source` loader.
- Update runtime create/delete/modify sentinel checks from `node.json` to
  `node.toml`.
- Update legacy init/config reload code to read `node.toml`.
- Update live runtime comments from `node.json` to `node.toml`.
- Add/update focused `lpc-engine` tests that create, modify, and delete
  `node.toml`.

Out of scope:

- Do not convert `ProjectBuilder`, CLI templates, server templates, or examples.
- Do not keep a long-term `node.json` fallback.
- Do not change concrete legacy runtime behavior.
- Do not touch M2.1 runtime-product code.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public entry points and abstract helpers before low-level helpers.
- Keep helper functions near the bottom of files and tests at the bottom.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment with a clear follow-up.

## Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to hide problems.
- Do not disable, skip, or weaken existing tests.
- If blocked by missing Phase 1 APIs or an ambiguous design issue, stop and
  report instead of improvising.
- Report back: files changed, validation run, validation result, and deviations.

## Implementation Details

Read the shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-design.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/01-add-source-owned-node-toml-loader.md`

Relevant files:

- `lp-core/lpc-engine/src/legacy_project/legacy_loader.rs`
- `lp-core/lpc-engine/src/legacy_project/project_runtime/core.rs`
- `lp-core/lpc-engine/src/legacy_project/mod.rs`
- `lp-core/lpc-engine/src/legacy/project.rs`
- `lp-core/lpc-engine/src/nodes/node_runtime.rs`
- `lp-core/lpc-engine/tests/scene_update.rs`
- `lp-core/lpc-engine/tests/partial_state_updates.rs`

`legacy_loader.rs` currently owns directory suffix detection and JSON parse
logic. After Phase 1, it should become a thin engine-facing adapter around the
source loader:

- preserve public functions used by `lpc-engine` if that keeps the diff small:
  `discover_nodes`, `legacy_load_node`, `legacy_node_kind_from_path`,
  `legacy_is_node_directory`;
- delegate their logic to `lpc_source::legacy` helpers;
- map source loader errors into `lpc_engine::error::Error`.

When mapping errors, preserve useful paths in `Error::Io`, `Error::Parse`, or
`Error::InvalidConfig`. Do not expose source loader internals in user-facing
messages if a concise message is enough.

Update `project_runtime/core.rs`:

- deletion sentinel should be `/node.toml`;
- comments/examples should mention `node.toml`;
- `extract_node_path_from_file_path` examples should mention `node.toml`;
- create handling can continue to detect node directories by suffix.

Update `legacy/project.rs`:

- all init-time config re-reads should use `node.toml`;
- config modify handling should trigger on `/node.toml`;
- reload should call the source-backed `legacy_load_node`.

The config reload path currently reads and parses the same config twice. Do not
refactor that behavior beyond what is necessary for `node.toml`; keep runtime
behavior stable.

Tests:

- Update `scene_update` to modify `/src/shader-1.shader/node.toml` using TOML
  syntax.
- Update deletion test to delete `node.toml`.
- Update `partial_state_updates` to write fixture config TOML.
- If direct TOML construction is verbose, use `toml::to_string` on the config
  structs already imported by the tests.

Do not convert `ProjectBuilder` in this phase. It may make some existing
runtime tests fail until Phase 3. If that happens, report it clearly and ensure
the direct loader tests in this phase pass.

## Validate

Run from the repository root:

```bash
cargo test -p lpc-engine --test scene_update --test partial_state_updates
```

If broader `lpc-engine` tests fail only because builders still write
`node.json`, note that Phase 3 is expected to address it.

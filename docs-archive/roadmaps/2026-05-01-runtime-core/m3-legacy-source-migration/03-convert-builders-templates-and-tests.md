# Phase 3: Convert Builders, Templates, and Tests

## Metadata

- **sub-agent:** yes
- **model:** composer-2
- **parallel:** 4

## Scope of Phase

Convert generated project files and live tests from `node.json` to `node.toml`
after the runtime loader has switched to the new sentinel.

In scope:

- Update `lpc-shared::ProjectBuilder` to write `node.toml`.
- Update `lp-cli create` to write `node.toml` and update its tests.
- Update `lpa-server` project template creation to write current TOML configs.
- Update server tests that copy project files to copy `node.toml`.
- Update live code/test comments in these files from `node.json` to
  `node.toml`.

Out of scope:

- Do not convert `examples/**`; Phase 4 owns examples.
- Do not change runtime loader behavior; Phase 2 owns runtime loading.
- Do not add JSON compatibility.
- Do not refactor project creation beyond the sentinel/serialization change.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public entry points and tests in the existing order used by each file.
- Keep helper functions near the bottom of files and tests at the bottom.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment with a clear follow-up.

## Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to hide problems.
- Do not disable, skip, or weaken existing tests.
- If blocked by missing loader behavior from Phase 2, stop and report.
- Report back: files changed, validation run, validation result, and deviations.

## Implementation Details

Read the shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/00-design.md`

Relevant files:

- `lp-core/lpc-shared/src/project/builder.rs`
- `lp-cli/src/commands/create/project.rs`
- `lp-app/lpa-server/src/template.rs`
- `lp-app/lpa-server/tests/server_tick.rs`
- `lp-app/lpa-server/tests/stop_all_projects.rs`

Use TOML serialization through `toml::to_string` or `toml::to_string_pretty`
where available. Do not hand-write TOML strings for config structs unless the
target is a small test fixture and typed serialization is impractical.

`ProjectBuilder`:

- Change all generated node config paths from `<node-dir>/node.json` to
  `<node-dir>/node.toml`.
- Serialize existing config structs with TOML.
- Update error messages and comments.
- Keep shader GLSL file handling unchanged.

`lp-cli create`:

- Change generated config file paths and tests to `node.toml`.
- Use the existing legacy config structs:
  - `TextureConfig`
  - `ShaderConfig`
  - `OutputConfig`
  - `FixtureConfig`
- Preserve current project layout and GLSL content.
- Update assertions that check file existence/read content.

`lpa-server/src/template.rs`:

- Audit the existing inline JSON. It appears to use an older config shape
  (`$type`, `texture_id`, etc.) rather than the current structs.
- Replace inline JSON with typed legacy config structs and TOML serialization.
- Keep paths and GLSL content stable unless the current paths are inconsistent
  with the current config structs.

Server tests:

- Replace `node.json` copying with `node.toml` copying.
- Keep GLSL copying unchanged.

Tests should continue to exercise the same behavior; only the source format
changes.

## Validate

Run from the repository root:

```bash
cargo test -p lpc-shared
cargo test -p lp-cli
cargo test -p lpa-server
```

# Phase 7: Migrate Remaining Tests, Examples, CLI, And Server Touchpoints

## Scope of phase

After the new loader is validated on `examples/basic`, migrate remaining repo assumptions from `/project.json` and discovered `/src` node directories to `/project.toml` and declared node artifacts.

In scope:

- Update remaining examples/tests that should use the new project-artifact layout.
- Update CLI/server validation and template paths that assume `project.json`.
- Update project creation/template code to create `project.toml` and node artifact files where in scope.
- Update docs/comments touched by these migrations.
- Keep changes focused on initial load and examples/tests.

Out of scope:

- Do not implement source reload.
- Do not redesign project manager UX beyond path/name assumptions needed for tests.
- Do not remove legacy compatibility projection.

## Code Organization Reminders

- Follow the repo rule: top to bottom is most important to least important, with tests at the bottom of each Rust file.
- Prefer one concept per file and keep related functionality grouped together.
- Keep helper functions below the public/primary API they support.
- Any temporary code must have a searchable TODO comment and should be removed by the cleanup phase.
- Preserve no_std compatibility in `lpc-model`, `lpc-source`, `lpc-engine`, and shader/runtime paths. Do not add std gates to compile/execute paths.

## Codex / Worker Reminders

- Do not commit. The plan commits at the end as a single unit unless the user explicitly says otherwise.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to make the build pass. Fix the issue.
- Do not disable, skip, or weaken existing tests.
- If blocked by ambiguity or an unexpected design issue, stop and report back rather than improvising.
- Report back with: what changed, what was validated, and any deviations from this phase.

## Implementation Details

Likely files from current `rg` output:

- `lp-app/lpa-server/src/project.rs`
- `lp-app/lpa-server/src/project_manager.rs`
- `lp-app/lpa-server/src/template.rs`
- `lp-cli/src/commands/create/project.rs`
- `lp-cli/src/commands/dev/validation.rs`
- `lp-cli/src/commands/profile/handler.rs`
- `lp-core/lpc-shared/src/project/builder.rs`
- `lp-core/lpc-engine/tests/*`
- Remaining `examples/*` that need to compile or run in tests

Use `rg` to build the exact checklist:

```bash
rg -n "project.json|/src/|discover_legacy_node_dirs|tree_path_for_legacy_src_dir|legacy_src" -g '*.rs' -g '*.md' -g '*.toml' -g '*.json'
```

Not every historical doc hit needs migration. Prioritize compiling code, tests, active examples, and user-facing templates.

## Validate

Start focused:

```bash
cargo test -p lpc-engine
cargo test -p lpa-server --no-run
cargo test -p lp-cli --no-run
```

If package names differ or tests are too broad for the current checkpoint, report exactly what was run and what remains.

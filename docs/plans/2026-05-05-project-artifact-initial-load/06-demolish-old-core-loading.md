# Phase 6: Demolish Old Core Initial-Load Path

## Scope of phase

Remove old core initial-load machinery after the new `/project.toml` loader is working for `examples/basic`.

In scope:

- Remove `/project.json` parsing from `CoreProjectLoader`.
- Remove `/src` directory discovery and suffix-derived node path logic from the core initial-load path.
- Remove `legacy_src_dirs` and related lookup APIs from `CoreProjectRuntime` if no longer needed.
- Delete or rewrite tests whose only purpose was directory discovery in the core loader.
- Keep truly legacy modules only where still used by older runtime paths or tests outside the core loader.

Out of scope:

- Do not remove compatibility wire structs.
- Do not remove old legacy runtime modules unless they are proven dead and unrelated to broader test migration.
- Do not migrate every CLI/server caller in this phase; that is Phase 7.

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

Relevant files:

- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-source/src/legacy/node_loader.rs`
- `lp-core/lpc-source/src/legacy/node_config_file.rs`
- `lp-core/lpc-engine/src/legacy_project/*` if compile references require updates

Use `rg` to find stale core-loading references:

```bash
rg -n "project.json|discover_legacy_node_dirs|legacy_src_dirs|tree_path_for_legacy_src_dir|/src" lp-core/lpc-engine lp-core/lpc-source
```

Be careful: not every hit should be deleted. Some docs/tests may remain until Phase 7, but core initial load must no longer depend on them.

## Validate

```bash
cargo test -p lpc-engine --test scene_render --test partial_state_updates --test get_changes_resource_projection
cargo test -p lpc-source
```

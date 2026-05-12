# Phase 3: Build Source Project To Engine Loader

## Scope of Phase

Add the first source project -> core `Engine` construction path for the current
authored legacy project layout. The loader may create placeholder node ports if
real node behavior is not available yet, but it must establish the durable
construction boundary for later phases.

In scope:

- Add `project_loader` under `project_runtime`.
- Load current `/project.json` and `/src/*.kind/node.toml` layout.
- Build a core `Engine` with a `NodeTree`.
- Create per-kind core node placeholders or factory hooks.
- Mark fixture nodes as demand roots.
- Add tests using `ProjectBuilder` / `LpFsMemory`.

Out of scope:

- Real shader execution.
- Real fixture sampling/output flushing.
- Server wiring.
- Replacing all legacy source shapes.

## Code Organization Reminders

- Keep loader and node factory concerns separate.
- Avoid ad hoc path parsing if existing `lpc-source` helpers exist.
- Tests live at the bottom of the relevant module or in focused integration
  tests.
- Record any tactical shortcut in `future.md`.

## Sub-agent Reminders

- Do not commit.
- Stay within phase scope.
- Do not suppress warnings or weaken tests.
- If source loading APIs cannot support the plan without a public API change,
  stop and report.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `lp-core/lpc-engine/src/project_runtime/` from Phase 1.
- `lp-core/lpc-engine/src/legacy_project/legacy_loader.rs`.
- `lp-core/lpc-source/src/legacy/node_loader.rs`.
- `lp-core/lpc-source/src/legacy/node_config_file.rs`.
- `lp-core/lpc-shared/src/project/builder.rs`.
- `lp-core/lpc-engine/src/tree/node_tree.rs`.
- `lp-core/lpc-engine/src/engine/test_support.rs`.

Expected API shape:

```rust
pub struct CoreProjectLoader;

impl CoreProjectLoader {
    pub fn load_from_root<R>(
        root: &R,
        services: RuntimeServices,
    ) -> Result<CoreProjectRuntime, CoreProjectLoadError>
    where
        R: LegacyNodeReadRoot + ?Sized;
}
```

Exact generic bounds may differ based on existing filesystem traits. Prefer
existing loader traits over adding a direct `lpfs` dependency if that would
create cycles.

Build behavior:

- Create `CoreProjectRuntime`.
- Add source nodes into the engine tree.
- Preserve enough identity information for tests to find nodes by kind/path.
- Add fixture nodes as demand roots.
- Do not rely on old `LegacyProjectRuntime` for construction.

If real node implementations do not exist yet, create minimal core placeholder
nodes in the planned `nodes::core` module only if needed for compile/tests.
Those placeholders must be clearly scoped and either useful as future shells or
recorded in `future.md`.

Tests:

- A `ProjectBuilder` project loads into `CoreProjectRuntime`.
- The loaded engine contains texture/shader/fixture/output nodes.
- Fixture nodes are demand roots.
- Loading malformed/missing node configs returns a structured error.

## Validate

Run:

```bash
cargo test -p lpc-engine project_runtime
```

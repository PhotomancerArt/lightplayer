# Phase 3: Implement Project Artifact Loading

## Scope of phase

Rewrite the core project initial-load path around `/project.toml` and artifact-loaded node definitions.

In scope:

- Add a new core loader entry point that starts from an `ArtifactSpecifier`, defaulting to `/project.toml` for project-root loads.
- Load `ProjectDef` using artifact loading infrastructure.
- Instantiate or attach the root `ProjectNode` / project placeholder for `kind = "project"`.
- Load child node artifacts declared in `ProjectDef.nodes`.
- Build a project-local name index for declared nodes.
- Populate compatibility authoring snapshots from loaded defs.

Out of scope:

- Do not support source reload or filesystem change handling.
- Do not preserve `/project.json` or `/src` discovery in the new core initial-load path.
- Do not migrate every caller yet; keep changes focused enough for the basic runtime smoke to proceed.

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
- `lp-core/lpc-engine/src/project_runtime/compatibility_projection.rs`
- `lp-core/lpc-engine/src/artifact/source_loader.rs`
- `lp-core/lpc-engine/src/artifact/artifact_location.rs`
- `lp-core/lpc-engine/src/nodes/core/placeholder.rs` or a new project node file if warranted

The new loader should conceptually do:

```text
load_project_artifact(fs, services, ArtifactSpecifier::path("/project.toml"))
  -> resolve/load ProjectDef
  -> create CoreProjectRuntime with services.project_root()
  -> attach ProjectNode/root payload
  -> read ProjectDef.nodes into a map: NodeName -> NodeInvocation
  -> load each child artifact into its concrete *Def
  -> create tree entries for children under root using the project-local names
```

This phase may initially create entries and attach nodes using a straightforward dependency order or a temporary hard-coded order for current node kinds if Phase 4 will generalize dependency handling. If you use a temporary order, leave a TODO that Phase 4 removes.

Do not use directory suffixes to determine kind. The loaded TOML file's `kind` selects the concrete def/runtime node type.

Add focused tests for:

- Loading `examples/basic/project.toml` creates a runtime with root and the four expected child names.
- Missing `/project.toml` returns a clear load error.
- A child artifact with mismatched/missing `kind` returns a clear load error.

## Validate

```bash
cargo test -p lpc-engine project_loader
```

If filters are awkward, run:

```bash
cargo test -p lpc-engine --test scene_render --no-run
```

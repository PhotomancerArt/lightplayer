# Summary

## What was built

- Added the `nodes.<name>.def` invocation boundary with `def = { path = ... }`
  and inline `[nodes.<name>.def] kind = ...` node definitions.
- Added first-class shader source specs with `source = { path = ... }` and
  `source = { glsl = ... }` for visual and compute shader nodes.
- Removed authored support for legacy `[nodes.x] artifact = ...` and
  `glsl_path = ...`; tests now assert those shapes are rejected.
- Updated project loading so path and inline node defs, plus path and inline
  GLSL sources, go through the same loader boundary.
- Restored live file updates in `lpa-server` by rebuilding the loaded project
  through `ProjectLoader::load_from_root` when filesystem changes arrive.
- Updated examples, templates, CLI project creation, wire sync tests, and the
  slot mockup to the new authored model.
- Added `docs/design/source-artifacts.md` to document the authored forms,
  current reload behavior, and the next finer-grained source resolver step.

## Decisions for future reference

#### Invocation Namespace

- **Decision:** Child node invocations use a `def` field for the node definition.
- **Why:** It leaves the rest of `[nodes.<name>]` available for future
  invocation-level bindings, overrides, and metadata.
- **Rejected alternatives:** Reusing top-level `artifact`; accepting both old
  and new shapes.

#### Shader Source Spec

- **Decision:** Shader source is an external enum under `source`, with `path`
  and `glsl` variants.
- **Why:** `path` is human-readable for relative authored references, and
  `glsl` leaves room for future sibling languages such as `wgsl`.
- **Rejected alternatives:** Generic `artifact`/`inline` field names;
  retaining `glsl_path`.

#### Reload Scope

- **Decision:** The server reloads the project through the canonical loader on
  filesystem changes.
- **Why:** It restores correct live updates for node definitions and GLSL now,
  without teaching shader nodes about files.
- **Rejected alternatives:** Continuing to call the no-op engine
  `handle_fs_changes` hook; wiring a partial shader-specific reload hook.
- **Revisit when:** Whole-project reload becomes too expensive and the engine
  needs a fine-grained versioned source resolver.

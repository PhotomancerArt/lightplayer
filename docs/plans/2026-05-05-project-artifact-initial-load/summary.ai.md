# Summary

## What Was Built

- Added project artifacts as the root authored entry point: `project.toml` now carries `kind = "project"` and a `[nodes.*]` map of named node invocations.
- Introduced source-side `Def` types for authored node bodies: `ProjectDef`, `TextureDef`, `ShaderDef`, `OutputDef`, and `FixtureDef`.
- Introduced `NodeInvocation` for "use this node here" authoring, currently backed by artifact specifiers and shaped for future params/bindings.
- Added relative dot `NodeLoc` parsing in `lpc-model` and loader-side resolution for sibling/current/parent references.
- Reworked `CoreProjectLoader` to start from `/project.toml`, load declared artifact files, build the root `Project` node, and attach declared child nodes without directory discovery.
- Flattened the active examples to file-referenced artifacts: `project.toml`, node `.toml` files, and `shader.glsl`.
- Updated project builders, server templates, CLI create/dev/profile paths, and integration fixtures to use the new project artifact layout.
- Removed the old engine project runtime and source directory discovery loader so the core runtime no longer has a second project-loading model.

## Decisions For Future Reference

#### Project Artifacts Are The Runtime Entry Point

- **Decision:** initialize the core runtime from a project artifact spec, resolving `/project.toml` as the implied root.
- **Why:** this makes project structure explicit and removes filesystem naming conventions from node discovery.
- **Rejected alternatives:** scanning `/src`, discovering directories by suffix, and keeping `/project.json` as a separate non-artifact root.

#### Artifacts Are Reusable Node Definitions

- **Decision:** artifacts are authored node definitions with a load location; project node entries invoke them by locator.
- **Why:** this lets a node definition live inline later or in its own file now, while keeping the runtime tree separate from the authored source files.
- **Rejected alternatives:** treating shader files or node directories as runtime nodes directly.

#### `Def`, `Invocation`, `Locator`, `Ref`

- **Decision:** use `*Def` for authored node bodies, `NodeInvocation` for a node used at a place in a project, `ArtifactSpecifier` for source-side external locations, and reserve `Ref` language for runtime references.
- **Why:** this keeps the "what is defined" and "where it is used" concepts distinct without overloading `Spec`.
- **Rejected alternatives:** `NodeSpec` as both path reference and full definition, and `ArtifactRef` for source authoring paths.

#### Relative Dot `NodeLoc`

- **Decision:** source node locations are relative dot strings for now, such as `.`, `.child`, and `..sibling`.
- **Why:** slash syntax was too easy to confuse with filesystem paths, and relative-only references match current authoring needs.
- **Rejected alternatives:** slash paths, absolute node tree paths, and keyword namespaces like `self`/`parent`.

#### Compatibility Wire Stays Temporary

- **Decision:** the runtime still projects into the legacy wire detail/state shapes for current client compatibility.
- **Why:** this plan is about initial project artifact loading, not the future fully dynamic data/wire model.
- **Future direction:** move toward general dynamic node namespaces such as `#config`, `#param`, and `#state`, with client-side ergonomic helpers layered above that data model.

## Validation

- `cargo fmt`
- `cargo test -p lpc-model`
- `cargo test -p lpc-source`
- `cargo test -p lpfs`
- `cargo test -p lpc-shared project_builder`
- `cargo test -p lpc-engine`
- `cargo test -p lpa-server`
- `cargo test -p lp-cli`
- `cargo test -p lp-cli --no-run`

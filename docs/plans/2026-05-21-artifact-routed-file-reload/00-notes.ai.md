# Artifact-Routed File Reload Notes

## Scope

Implement file-change reload so a running LightPlayer project updates only the artifacts and nodes affected by changed files.

The target architecture is:

- File changes are routed into the running engine.
- The artifact manager owns file identity, content freshness, and load error state.
- Nodes react to changed artifacts in place.
- File changes must not call `Project::reload()` or reconstruct the whole `Engine`.
- Hot reload must avoid peak-memory patterns that keep an old whole-engine runtime alive while constructing a new one.
- Source-file artifacts must not retain full file contents in memory. They should track only identity and freshness for now: file path/name plus content version. Nodes lazy-load actual bytes/text only while preparing or compiling.

This plan covers:

- Replacing the server file-change path that currently calls `Project::reload()`.
- Clarifying the artifact model before building reload on top of it.
- Extending `ArtifactStore` so source files such as GLSL and SVG are normal file-backed artifacts, not special server reload cases, while keeping their contents out of long-lived heap storage.
- Separating parsed node definitions from artifacts if needed, so artifact storage does not become a confusing node definition registry.
- Registering node-to-artifact dependencies during project load.
- Letting shader and fixture nodes respond to changed dependent artifacts.
- Handling node TOML changes by reloading/repreparing affected nodes in place.
- Capturing bad source/TOML as artifact and node error state while keeping last-good runtime state where possible.
- Preserving scarce ESP32 heap by avoiding cached source text, duplicate engines, and unnecessary duplicate node runtimes.

This plan does not cover:

- A full optimal graph diff for arbitrary `project.toml` edits in the first implementation.
- Library artifact specifiers.
- Host precompilation or any change that weakens the on-device GLSL JIT path.

## Current State

Relevant files:

- `lp-app/lpa-server/src/server.rs`
  - `LpServer::tick` collects project-relative `FsChange`s.
  - Current behavior calls `project.reload()` for project changes.
  - This is the wrong architectural boundary for file reload.

- `lp-app/lpa-server/src/project.rs`
  - `Project::reload()` drops the old runtime and calls `ProjectLoader::load_from_root`.
  - This remains useful for explicit user-initiated full reload/load flows, but should not run on filesystem changes.

- `lp-core/lpc-engine/src/engine/engine.rs`
  - `Engine::handle_fs_changes(&mut self, _changes: &[FsChange])` exists but is a no-op.
  - `Engine` owns `ArtifactStore`, `artifact_nodes`, `demand_roots`, and runtime node tree.
  - `TickContext` currently receives the owning node definition artifact id and `content_frame`.

- `lp-core/lpc-engine/src/artifact/*`
  - `ArtifactStore` maps `ArtifactLocation` to `ArtifactId`.
- `ArtifactLocation::File(LpPathBuf)` already exists.
- `ArtifactLocation::InlineNode { owner, name }` exists, but this is likely a model smell. Inline node definitions are not artifacts; they are node definitions derived from an owning artifact such as `project.toml` or a playlist node definition.
- `ArtifactState` currently stores only `NodeDef` payloads.
- `ArtifactStore::load_with` overwrites state with an error on load failure.
- There is no generic text/source-file payload.
- There is no last-good payload plus latest-error model.
- There is no content fingerprint/hash/diff API. Do not add one in the first pass.
- The filesystem abstraction currently exposes `read_file`, which returns a `Vec<u8>`. Avoid using it in artifact freshness handling unless a node is actively preparing and will drop the buffer immediately.

- `lp-core/lpc-engine/src/engine/project_loader.rs`
- Node TOML files are loaded into `ArtifactStore` as `NodeDef`.
  - `ShaderSource::Path` is read by `read_shader_source`, producing a `String` passed to `ShaderNode::new`.
  - `MappingConfig::SvgPath` is read by `resolve_fixture_mapping`, producing a resolved `MappingConfig` passed to `FixtureNode::new`.
  - GLSL and SVG files are therefore not tracked as artifacts after project load.

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
  - Shader node already syncs authored shader config from node def slots during `produce`.
- Shader source text is currently stored directly as `glsl_source`.
- Compile failures are stored in `compilation_error`.
- Current compile failure drops `self.shader`; for hot reload we should stage new compiles so last-good compiled shader can remain active when a changed source fails.
- For path-based shader sources, long-lived `glsl_source: String` is wasteful. The node should store a source artifact reference and last-seen source frame, read source text only during compilation, and drop it after compile succeeds or fails.

- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
  - Fixture node already syncs mutable authored mapping fields for `PathPoints`.
  - `SvgPath` is currently ignored by runtime sync because it is resolved at load time.
- Runtime holds precomputed/direct mapping state derived from the resolved mapping.
- For SVG mappings, the SVG text should be read only while resolving a candidate mapping. The long-lived fixture state should keep the resolved/precomputed mapping, not the SVG text.

- `lp-core/lpc-engine/src/node/contexts.rs`
  - `TickContext::artifact_ref`, `artifact_content_frame`, and `artifact_changed_since` exist.
  - These only cover the owning node definition artifact, not dependent source artifacts.

- `lp-core/lpc-engine/src/engine/slot_mutation.rs`
- Slot mutation updates a node definition artifact in place and bumps `content_frame`.
- This is the closest existing example of the intended model: update artifact payload, let node runtime observe content frame and sync from authored slots.

## Model Correction

The current names and ownership boundaries are muddy:

- An artifact should be the thing loaded from outside the runtime graph: usually a file or future library resource.
- A `NodeDef` is derived from an artifact. It is not itself the artifact.
- Inline node definitions are also derived from an owning artifact. They should not require fake artifact locations.
- A running node entry should point to a node definition handle, and that handle should be backed by a node definition registry or table.
- The artifact store should answer "what source changed, and what version is it?"
- The node definition registry should answer "what parsed node definition does this node instance use, and when did that definition last change?"

This suggests a split:

- `ArtifactStore`: source identity and freshness. Minimal file artifacts store path plus content version. It should not assume payloads are only `NodeDef`.
- `NodeDefRegistry` or equivalent: parsed `NodeDef` storage, including definitions parsed from node TOML files and inline definitions parsed out of another artifact.
- `NodeDefHandle`: handle into the node definition registry, not an artifact id plus path pretending every node def is an artifact root.
- Dependency index: maps artifact ids to derived node def handles and live node ids.

The reload plan should start by making this model explicit enough that source-file reload does not add more special cases to `ArtifactStore`.

## User Notes

- File reloads should only reload what they need.
- Reload must be routed through the artifact manager.
- Server/file-watcher code should not know special cases for GLSL or SVG.
- GLSL and SVG should just be file artifacts.
- Artifacts are not meant to be only `NodeDef`s. The artifact is the source from which a `NodeDef` or source text comes.
- `ArtifactLocation::InlineNode` is probably wrong; inline node definitions belong in a node definition registry derived from the owning artifact.
- A separate `NodeDefRegistry` may be needed to clarify responsibilities.
- The running project should handle changes.
- Changes to node files should reload the affected node, not the whole project.
- It is acceptable initially to reload child nodes when a node file changes, but avoid unnecessary whole-project reload.
- ESP32 memory is tight; do not implement transactional whole-engine reload that holds two engines at once.
- 300 KB of heap is a real production constraint. The first real art project hit memory limits, so this design should prefer metadata, handles, and staged local work over retaining full files or duplicating runtime state.
- Keep the first artifact metadata model minimalist. Name/path and version are probably enough. Add digest, length, or richer read/stat metadata only when a real bug or performance problem demonstrates the need.
- Preserve the on-device GLSL JIT. Do not gate or stub the compiler path.

## Open Questions

### How should `project.toml` file changes behave in the first implementation?

- **Context:** Node TOML changes can be mapped to a specific loaded node through `artifact_nodes`. `project.toml` changes can add, remove, or rewire top-level nodes and may require graph reconciliation.
- **Suggested answer:** Do not call `Project::reload()`. Treat `project.toml` as the root graph artifact. Phase 1 should route it through artifact invalidation and record a root/project error state if live reconciliation is unsupported. A later phase can implement limited reconciliation for adding/removing/repointing top-level nodes.

### Should source file artifacts store bytes or UTF-8 text?

- **Context:** GLSL and SVG need UTF-8 text. Future resources may need bytes. `ArtifactState` currently stores only `NodeDef`.
- **Answer:** No for long-lived source files. Source-file artifacts should store metadata only: location/path and content version. Nodes lazy-load actual bytes/text from `ArtifactReadRoot` only while preparing/compiling. Node definitions remain stored as parsed `NodeDef` because the live graph reads their slots every frame.

### Should parsed node definitions remain in `ArtifactStore`?

- **Context:** `ArtifactStore` currently stores `NodeDef` directly and even has `ArtifactLocation::InlineNode`, which makes it act like a node definition registry. The user clarified that an artifact is the thing from which a node def comes, not the node def itself.
- **Suggested answer:** Introduce a separate `NodeDefRegistry` or equivalent table. Keep parsed `NodeDef`s there. Artifacts should track source freshness and maybe load/read errors. The registry should track derived definitions and their source artifact/version.

### What replaces `ArtifactLocation::InlineNode`?

- **Context:** Inline nodes are authored inside another artifact, such as `project.toml` or a playlist/node definition file. They need stable definition handles but are not separately loadable files.
- **Suggested answer:** Represent inline definitions as `NodeDefHandle`s in `NodeDefRegistry`, with metadata pointing to their owning source artifact plus an inline path/name. Do not represent them as artifact locations.

### Should failed hot reload keep last-good payload?

- **Context:** `ArtifactStore::load_with` currently replaces state with error on failure. For hot reload, replacing the only good payload would make dependent nodes lose working state.
- **Suggested answer:** Yes. Store last-good payload and latest error separately. Load failure updates error metadata and error frame, not the last-good payload/content frame.

### How should artifact diffing work without retaining full file contents?

- **Context:** We might eventually want unchanged writes to avoid bumping `content_frame`, but retaining or reading full source text for generic diffing is too expensive for this first pass.
- **Answer:** Do not solve byte-level diffing in the first pass. Treat filesystem change events as version bumps for known file artifacts. If duplicate writes become noisy later, add a cheap digest/length or filesystem-provided version/stat layer as a follow-up.

### Should changed GLSL/SVG immediately reprepare nodes, or should nodes notice on next tick/render?

- **Context:** Demand roots tick every frame, but shader compile happens during render, not produce. Fixture control render happens in control rendering. Immediate reprepare gives faster error readback but can do expensive work in the file-change handler.
- **Suggested answer:** Artifact invalidation/checking happens immediately and updates metadata only. Nodes check dependent artifact frames during their normal produce/render/control paths and stage work there. The engine may mark affected nodes dirty/error-readable immediately when artifact stat/read/hash fails.

## Suggested Plan Name

`2026-05-21-artifact-routed-file-reload`

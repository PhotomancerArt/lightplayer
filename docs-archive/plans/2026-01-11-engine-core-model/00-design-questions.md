# Design Questions: Engine Core and Model Refactor

## Context

We're refactoring `lp-engine` to have a cleaner architecture. The old code has been moved to `src.bk` for reference. We need to:

1. Move shared config and state model from `lp-shared` to `lp-model`
2. Create initial `ProjectRuntime` code and structure
3. Support loading a project, initializing nodes, and rendering a simple scene

## Questions

1. **Path representation**: The design doc mentions `LpPath` (no-std path). Should this be:
   - A newtype wrapper around `String`?
   - A more structured type with path operations?
   - Should it support relative vs absolute paths, or always absolute from project root?
   
   **ANSWER**: Design to support relative paths later (nodes will be nestable), but start with absolute paths only. Use a newtype wrapper around `String` that can be extended later.

2. **Node specifier resolution**: The design shows `NodeSpecifier::Path(String)` for resolving nodes. Should this support:
   - Relative paths (e.g., `"../texture.texture"`)?
   - Absolute paths (e.g., `"/src/texture.texture"`)?
   - Both?
   
   **ANSWER**: `NodeSpecifier` should be a newtype wrapper around `String` for now. It's semantically unique and may support other kinds of specifiers in the future. Currently it's just a path string.

3. **Handle generation**: The design shows handles generated during `loadNodes()`. Should handles be:
   - Stable across reloads (same path = same handle)?
   - Or can they change (new handle each time)?
   
   **ANSWER**: Handles are runtime identifiers and can change on reload. Sequential generation (as in old code) is fine. Paths are for loading/resolving at load/config time; handles are for runtime references.

4. **NodeEntry lifecycle**: The design shows `NodeEntry` with `status`, `runtime`, and version tracking. When a node fails to initialize:
   - Should we keep the entry with `status: InitError` and `runtime: None`?
   - Or remove it entirely?
   
   **ANSWER**: Keep failed nodes in the map with error status. Even missing node.json should create an entry with error. This makes errors visible in UI and allows node references to resolve to failed nodes (giving better error messages like "node not initialized" instead of "node not found"). Good error handling is a very high priority.

5. **Filesystem abstraction**: Should `ProjectRuntime` own the `LpFs`, or receive it as a parameter for operations? The old code passed it around; the design shows it as a field.
   
   **ANSWER**: Uncertain - may make sense for ProjectRuntime to own it as mut. The fs abstraction may need unsafe code at some point as it's a tricky abstraction. Revisit during design discussion.

6. **Texture rendering trigger**: The design says textures are rendered when `state_ver < frame_id` via shaders. Should this be:
   - Lazy (only when requested via `get_texture()`)?
   - Or eager (all textures updated each frame)?
   
   **ANSWER**: Yes, lazy rendering is important - only render textures when requested via `get_texture()`.

7. **Shader priority**: The design mentions ordering shaders by priority. Should priority be:
   - A numeric field in shader config?
   - Or based on some other ordering (e.g., dependency order)?
   
   **ANSWER**: Use a numeric field called "render_order" in shader config. Lower numbers render first (opposite of typical priority). Default is 0.

8. **Error handling**: Should node initialization errors be:
   - Collected and reported, but not block other nodes?
   - Or should the first error stop the process?
   
   **ANSWER**: Don't stop the process. All processing should be done at the node level with "show must go on" philosophy - allow users to get the best view of the overall state of the system.

9. **Testing approach**: For the unit test goal (load project, initialize nodes, render simple scene), should we:
   - Use the in-memory filesystem (`LpFsMemory`)?
   - Or create a test helper that builds projects programmatically?
   
   **ANSWER**: Use a whole test-data building pattern. Functions that generate pre-built builders for testing with basic nodes would be useful (which can then be customized per test).

10. **Node config vs runtime state**: The design separates config (from `node.json`) and state (shared with clients). Should the initial state be:
   - Derived from config during init?
   - Or explicitly set in config?
   
   **ANSWER**: Config sets initial values and "how the state is computed" (e.g., file paths, expressions, references). State contains actual runtime values (e.g., actual GLSL code loaded from file). Runtime should almost fully replicate config, but they serve different purposes. In the future, configs may reference other nodes (expressions), but state always has the current used value. Initial state is derived from config during init.

11. **FrameId type**: Should `FrameId` be:
    - A newtype wrapper around `i64` (as in old code)?
    - Or something else?
    
    **ANSWER**: Yes, newtype wrapper around `i64` (as in old code). Should be in `lp-model`.

12. **Node kind vs node type**: The design uses `NodeKind` enum. Should this match the node directory suffixes (`.shader`, `.texture`, `.output`, `.fixture`), or be more abstract?
    
    **ANSWER**: Yes, `NodeKind` should match the node directory suffixes (`.shader`, `.texture`, `.output`, `.fixture`).

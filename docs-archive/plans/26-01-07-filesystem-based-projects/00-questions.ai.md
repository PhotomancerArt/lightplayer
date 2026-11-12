# Questions: Filesystem-Based Projects

## Current State

Currently, `lp-core` and `fw-host` work with a single `project.json` file that contains:
- Top-level project config (uid, name)
- All nodes in nested `nodes` object with numeric IDs (u32)
- Nodes are stored as `HashMap<u32, NodeType>` for each node type

`fw-host` loads projects from `"project.json"` in the current directory.

The `Filesystem` trait currently provides:
- `read_file(path: &str)`
- `write_file(path: &str, data: &[u8])`
- `file_exists(path: &str)`

## Proposed Changes

1. **Project Structure**: Projects become directories with:
   - `project.json` (or `lp-project.json`?) at the root - contains only top-level config
   - `src/` directory containing node directories
   - Each node directory contains `node.json` + node-specific files (e.g., `main.glsl` for shaders)

2. **Node IDs**: Change from numeric IDs (u32) to path-based IDs (strings like "shader1")

3. **Filesystem Abstraction**: Add directory listing capability

4. **File Change Detection**: Pass list of changed file paths to `tick()` function

5. **fw-host**: 
   - Store projects in `projects/[name]/` directory
   - Accept project directory path as argument
   - Support `--create` flag to initialize new project
   - Log file changes

## Questions

1. **Project File Name**: Should the project config file be named `project.json` or `lp-project.json`? The user mentioned `lp-project.json` in the description, but existing code uses `project.json`.

2. **Node ID Format**: ✅ **DECIDED**: 
   - Use path-based IDs relative to `src/` (e.g., "shader1", "shaders/my-shader")
   - Support nested directories
   - Node IDs should be lowercase and kebab-case
   - Case-sensitive but enforce lowercase where possible

3. **Node JSON Structure**: ✅ **DECIDED**: Minimal - just the node config (same structure as currently), no duplicated metadata

4. **Shader GLSL Files**: ✅ **DECIDED**: 
   - Default to using a separate `main.glsl` file
   - May allow configuring the filename later, but for now use `main.glsl`
   - Shader node config in `node.json` should reference/point to the GLSL file (or we load it automatically from `main.glsl` in the same directory)

5. **Filesystem Change Detection**: ✅ **DECIDED**:
   - Use an enum for change types: `Create`, `Modify`, `Delete`
   - All paths (including changed paths) should be relative to project root
   - This enables testing with fake filesystem abstractions (e.g., HashMap-backed)
   - `tick()` accepts a list of file changes with paths relative to project root
   - `fw-host` uses filesystem watching APIs to detect changes
   - When a `node.json` changes, reload that node (and check related files like `main.glsl`)

6. **Project Directory Structure**: ✅ **DECIDED**:
   - `fw-host` stores projects in `projects/[name]/` by default
   - Allow loading from any directory that contains `project.json`
   - Support `--create` flag to initialize new project structure
   - Accept project directory path as command-line argument
   - If no project is specified, use an in-memory filesystem with a sample project (testing mode)

7. **Migration**: ✅ **DECIDED**: No backward compatibility - require new format immediately (still in dev mode)

8. **Node Type Detection**: ✅ **DECIDED**:
   - Use directory name suffix to indicate node category: `.shader`, `.texture`, `.output`, `.fixture`
   - Node IDs use full path from project root, INCLUDING leading slash and suffix (e.g., `src/my-shader.shader/` → ID is `"/src/my-shader.shader"`)
   - This provides 1:1 mapping from filesystem path to ID (advantages for future non-local nodes)
   - `$type` in `node.json` indicates the type WITHIN the category (e.g., `Single` for shaders, `GpioStrip` for outputs)
   - Discover nodes by recursively scanning `src/` for directories ending in these suffixes
   - Format: `/[path]/[id].[category]` (e.g., `"/src/my-shader.shader"`, `"/src/nested/effects/rainbow.shader"`)


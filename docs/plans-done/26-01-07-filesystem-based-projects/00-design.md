# Design: Filesystem-Based Projects

## Overview

Transform `lp-core` to work with filesystem-based projects where each node is stored in its own directory. This enables IDE-based editing and real-time file watching for development.

## File Structure

```
lp-core/src/
├── fs/                            # NEW: Central filesystem abstraction module
│   ├── mod.rs
│   ├── trait.rs                   # Filesystem trait (moved from traits/filesystem.rs)
│   └── memory.rs                  # In-memory filesystem implementation for testing
├── project/
│   ├── config.rs                  # MODIFY: ProjectConfig (top-level only), add path-based IDs
│   ├── loader.rs                  # NEW: Load project from filesystem structure
│   └── runtime.rs                 # MODIFY: Handle path-based IDs, file change reloading
├── nodes/
│   ├── id.rs                      # MODIFY: Change IDs from u32 to String (path-based, full path with leading slash)
│   └── [node types]/               # MODIFY: Update to use String IDs
└── app/
    └── lp_app.rs                  # MODIFY: Add file change handling to tick()

fw-host/src/
├── fs/
│   └── host.rs                    # MODIFY: Host filesystem implementation (moved from filesystem.rs)
├── watcher.rs                     # NEW: Filesystem watcher using notify crate
└── main.rs                        # MODIFY: Add --create flag, project dir argument, file watching
```

## Code Structure

### New Types

**File Change Tracking:**
```rust
pub enum ChangeType {
    Create,
    Modify,
    Delete,
}

pub struct FileChange {
    pub path: String,  // Path relative to project root (e.g., "/src/my-shader.shader/main.glsl")
    pub change_type: ChangeType,
}
```

**In-Memory Filesystem (Testing):**
- `InMemoryFilesystem` - HashMap-backed filesystem for testing
- Stores files as `HashMap<String, Vec<u8>>` where keys are paths relative to project root
- **Change Tracking**: Tracks all file changes (create, modify, delete) since last checkpoint
- **Methods**:
  - `new()` - Create empty filesystem
  - `get_changes() -> Vec<FileChange>` - Get all changes since last call (clears change log)
  - `reset_changes()` - Clear change log without retrieving
- **Testing Workflow**:
  1. Create `InMemoryFilesystem`
  2. Write files (e.g., `write_file("/project.json", ...)`, `write_file("/src/shader.shader/node.json", ...)`)
  3. Create LpApp with the filesystem
  4. Mutate filesystem (modify/delete files)
  5. Call `get_changes()` to get `Vec<FileChange>`
  6. Pass changes to `tick()` 
  7. Validate project state matches expectations

### Modified Types

**ProjectConfig:**
- Remove `nodes` field - nodes are now loaded from filesystem
- Keep only top-level config: `uid`, `name`

**Node IDs:**
- Change from `u32` to `String`
- Format: Full path from project root with leading slash (e.g., `"/src/my-shader.shader"`, `"/src/nested/effects/rainbow.shader"`)
- Includes the category suffix (`.shader`, `.texture`, `.output`, `.fixture`)

**Filesystem Trait (Central Abstraction):**
```rust
pub trait Filesystem {
    /// Read a file from the filesystem
    /// Path is relative to project root (e.g., "/project.json", "/src/my-shader.shader/main.glsl")
    fn read_file(&self, path: &str) -> Result<Vec<u8>, Error>;
    
    /// Write data to a file in the filesystem
    /// Path is relative to project root
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), Error>;
    
    /// Check if a file exists in the filesystem
    /// Path is relative to project root
    fn file_exists(&self, path: &str) -> Result<bool, Error>;
    
    /// List directory contents (files and subdirectories)
    /// Path is relative to project root (e.g., "/src" or "/src/nested")
    /// Returns paths relative to project root
    fn list_dir(&self, path: &str) -> Result<Vec<String>, Error>;
}
```

**Path Semantics:**
- All paths are **relative to the project root**
- `/project.json` is always the project configuration file
- Leading slash indicates path from project root (e.g., `/src/my-shader.shader/main.glsl`)
- Filesystem instances have a root path (especially for real FS) to provide security by preventing access outside the project directory

**LpApp::tick():**
```rust
pub fn tick(
    &mut self,
    delta_ms: u32,
    incoming: &[MsgIn],
    file_changes: &[FileChange],  // NEW
) -> Result<Vec<MsgOut>, Error>
```

### New Functions

**ProjectLoader (new module):**
- `load_from_filesystem(fs: &dyn Filesystem) -> Result<ProjectConfig, Error>`
  - Scans `src/` recursively for directories ending in `.shader`, `.texture`, `.output`, `.fixture`
  - Loads each node from its directory
  - Returns `ProjectConfig` with top-level info only (nodes loaded separately into runtime)

- `load_node(fs: &dyn Filesystem, node_path: &str) -> Result<(NodeId, NodeConfig), Error>`
  - Loads a single node from its directory
  - Reads `node.json` and related files (e.g., `main.glsl` for shaders)
  - Returns node ID (full path) and config

**LpApp:**
- `handle_file_changes(&mut self, changes: &[FileChange]) -> Result<(), Error>`
  - Processes file changes
  - Reloads affected nodes (destroy and recreate)
  - Handles node creation/deletion

**HostFilesystem (Real Filesystem):**
- `new(root_path: PathBuf)` - Create filesystem instance locked to a specific root directory
- **Security**: All path operations are validated to ensure they stay within the root path
- `list_dir(path: &str) -> Result<Vec<String>, Error>`
  - Lists directory contents (files and subdirectories)
  - Returns paths relative to project root
- Paths are resolved relative to the root path, preventing access outside the project directory

## Project Structure

```
projects/[name]/
├── project.json                   # Top-level config only: { "uid": "...", "name": "..." }
└── src/
    ├── my-shader.shader/
    │   ├── node.json              # ShaderNode config (without glsl field)
    │   └── main.glsl               # GLSL source code
    ├── my-texture.texture/
    │   └── node.json              # TextureNode config
    └── nested/
        └── effects/
            └── rainbow.shader/    # Nested directories supported
                ├── node.json      # ID: "/src/nested/effects/rainbow.shader"
                └── main.glsl
```

## Node ID Format

- **Format**: `/[path]/[id].[category]`
- **Examples**:
  - `"/src/my-shader.shader"`
  - `"/src/nested/effects/rainbow.shader"`
  - `"/src/led-strip.output"`
- **Category suffixes**: `.shader`, `.texture`, `.output`, `.fixture`
- **Validation**: Directory suffix must match node type from `node.json` `$type` field

## File Change Handling

1. `fw-host` uses filesystem watcher (notify crate) to detect changes
2. Changes are passed to `LpApp::tick()` as `file_changes` parameter
3. `LpApp` processes changes:
   - **Create**: Load new node from filesystem
   - **Modify**: Reload node (destroy and recreate)
   - **Delete**: Remove node from runtime
4. File changes are logged

## Testing Mode

If no project directory is specified, `fw-host` uses an in-memory filesystem with a sample project. This allows easy testing without filesystem setup.

## Backward Compatibility

No backward compatibility - old single-file `project.json` format is not supported. Projects must use the new filesystem-based structure.


# Plan Overview: Filesystem-Based Projects

## Goal

Transform `lp-core` to work with filesystem-based projects where each node is stored in its own directory. This enables IDE-based editing and real-time file watching for development, making `fw-host` act like a simple app dev engine.

## Key Changes

1. **Project Structure**: Projects become directories with `project.json` (top-level config only) and `src/` containing node directories
2. **Node IDs**: Change from numeric IDs (`u32`) to path-based IDs (`String`) using full paths from project root (e.g., `"/src/my-shader.shader"`)
3. **Node Organization**: Each node in its own directory with `node.json` and related files (e.g., `main.glsl` for shaders)
4. **Filesystem Abstraction**: Central, well-organized filesystem abstraction with:
   - All paths relative to project root (`/project.json` is always the project file)
   - In-memory implementation with change tracking for easy testing
   - Host implementation with root path security
5. **File Change Detection**: Pass list of changed file paths to `tick()` function for real-time updates
6. **fw-host CLI**: Support `--create` flag and project directory path argument

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

## Filesystem Design Principles

1. **Central Abstraction**: Filesystem abstraction is central to the app and lives in `lp-core/src/fs/`
2. **Path Semantics**: All paths are relative to project root. `/project.json` is always the project file
3. **Security**: Filesystem instances have a root path (especially for real FS) to prevent access outside the project directory
4. **Testing**: In-memory filesystem tracks changes, making it easy to:
   - Save files
   - Create an lp project
   - Mutate the filesystem
   - Ask what changed (to send to `tick()`)
   - Validate that the project matches expectations

## Testing Mode

If no project directory is specified, `fw-host` uses an in-memory filesystem with a sample project. This allows easy testing without filesystem setup.

## Backward Compatibility

No backward compatibility - old single-file `project.json` format is not supported. Projects must use the new filesystem-based structure.


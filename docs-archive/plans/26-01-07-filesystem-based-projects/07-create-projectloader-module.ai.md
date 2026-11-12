# Phase 7: Create ProjectLoader Module

## Goal

Create a `ProjectLoader` module that loads projects from the filesystem structure.

## Tasks

1. Create `lp-core/src/project/loader.rs`:
   - `ProjectLoader` struct (or just functions)
   - `load_from_filesystem(fs: &dyn Filesystem) -> Result<ProjectConfig, Error>`:
     - Read `/project.json` to get top-level config
     - Scan `/src/` recursively for node directories (ending in `.shader`, `.texture`, `.output`, `.fixture`)
     - For now, just return the top-level config (node loading comes later)
   - `discover_nodes(fs: &dyn Filesystem) -> Result<Vec<String>, Error>`:
     - Recursively scan `/src/` for node directories
     - Return list of node paths (e.g., `["/src/my-shader.shader", "/src/nested/effects/rainbow.shader"]`)

2. Update `LpApp::load_project()`:
   - Use `ProjectLoader::load_from_filesystem()` instead of reading `project.json` directly
   - For now, don't load nodes yet (comes in next phase)

3. Add helper functions:
   - `is_node_directory(name: &str) -> bool` - check if directory name ends with node suffix
   - `extract_node_id(path: &str) -> Option<String>` - extract node ID from path

4. Add tests:
   - Test loading project.json
   - Test discovering nodes in src/
   - Test nested directories

## Success Criteria

- `ProjectLoader` module exists
- Can load top-level project config from filesystem
- Can discover node directories recursively
- Tests pass
- Code compiles without warnings


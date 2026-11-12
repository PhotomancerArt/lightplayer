# Phase 5: Create lp-server Library

## Goal

Implement the `lp-server` library for multi-project management.

## Tasks

1. **Create Project wrapper**:
   - Wraps `LpApp` instance with project metadata (name, path, etc.)
   - Provides access to the underlying `LpApp`
   - Handles project lifecycle (load, unload)

2. **Create ProjectManager**:
   - Manages multiple `Project` instances
   - Methods: `load_project(name)`, `unload_project(name)`, `get_project(name)`, `list_projects()`
   - Handles project filesystem paths

3. **Implement project creation logic**:
   - `create_project(name)` - creates filesystem structure
   - Creates project directory, `project.json`, initial node structure
   - Initializes `LpApp` for the new project
   - Returns `Project` instance

4. **Add error types**:
   - `lp-server/src/error.rs` with server-specific errors
   - Project not found, project already exists, etc.

5. **Update Cargo.toml**:
   - Add dependencies: `lp-core`, `lp-api`, `lp-core-util`
   - Add `std` feature support

6. **Basic implementation**:
   - Start with single-project support (can extend later)
   - Focus on project creation and loading

## Success Criteria

- `Project` wrapper implemented
- `ProjectManager` implemented with basic methods
- `create_project()` creates filesystem structure and initializes `LpApp`
- Code compiles without warnings
- Basic tests (optional but recommended)

# Phase 4: Implement LpApp::load_project()

## Goal

Implement project loading from filesystem.

## Tasks

1. Add `load_project(&mut self, path: &str) -> Result<(), Error>` to `LpApp`:
   - Read file from `platform.fs` using `read_file(path)`
   - Parse JSON to `ProjectConfig`
   - Create `ProjectRuntime` with project UID
   - Call `runtime.init(&config, &platform.output)` to initialize nodes
   - Store runtime in `self.runtime`
   - Handle errors gracefully (log, set status, etc.)

2. Handle case where project already loaded (destroy old runtime first)

3. Add error handling and logging

## Success Criteria

- `load_project()` compiles
- Can load project from filesystem
- Runtime is initialized with nodes
- Errors are handled gracefully
- Code compiles without warnings


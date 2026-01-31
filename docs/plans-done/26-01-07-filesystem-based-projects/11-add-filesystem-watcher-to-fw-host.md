# Phase 11: Add Filesystem Watcher to fw-host

## Goal

Add filesystem watching to `fw-host` to detect file changes in real-time.

## Tasks

1. Add `notify` crate dependency to `fw-host/Cargo.toml`

2. Create `fw-host/src/watcher.rs`:
   - `FileWatcher` struct wrapping `notify::RecommendedWatcher`
   - `watch_project(root_path: PathBuf) -> Result<FileWatcher, Error>`
   - `get_changes(&mut self) -> Vec<FileChange>`:
     - Collect file change events from watcher
     - Convert to `FileChange` format
     - Return paths relative to project root

3. Update `fw-host/src/main.rs`:
   - Create `FileWatcher` when project is loaded
   - In main loop, call `watcher.get_changes()` each frame
   - Pass changes to `lp_app.tick()`

4. Handle watcher events:
   - Convert `notify::Event` to `FileChange`
   - Map file paths to relative paths (from project root)
   - Filter out irrelevant events (e.g., temporary files)

5. Add error handling:
   - Handle watcher errors gracefully
   - Log file change events

## Success Criteria

- Filesystem watcher detects file changes
- Changes are converted to `FileChange` format
- Changes are passed to `lp_app.tick()`
- Code compiles without warnings
- File changes are logged


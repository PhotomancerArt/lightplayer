# Plan Notes: Local Filesystem Detection and Sync

## Context

The `dev` command currently has a `fs_loop` that's supposed to watch for file changes and sync them to the server, but it has a TODO comment and doesn't actually detect file changes - it just polls. We need to implement real filesystem watching.

**Current State**:
- `fs_loop` exists with debouncing logic but no actual file watching
- `sync_file_change` function exists and works correctly
- `notify` crate is already in dependencies (v6)
- There's a commented-out `watcher` module in `mod.rs`
- Tests exist for debouncing logic but not for actual file watching

**Goal**: Implement real-time filesystem watching that detects file changes and syncs them to the server.

## Questions

### Question 1: Should we use the existing notify dependency or check for alternatives?

**Context**: `notify` v6 is already in `Cargo.toml`. It's a well-maintained cross-platform file watching library.

**Suggested Answer**: Use `notify` v6. It's already a dependency, supports async via tokio, and is the standard solution for this problem.

### Question 2: Where should FileWatcher live?

**Context**: There's a commented-out `watcher` module in `mod.rs`. The `fs_loop` is in `commands/dev/fs_loop.rs`.

**Options**:
- A) Create `commands/dev/watcher.rs` (alongside `fs_loop.rs`)
- B) Put watcher logic directly in `fs_loop.rs`
- C) Create a shared watcher module elsewhere

**Suggested Answer**: Option A - Create `commands/dev/watcher.rs`. This keeps concerns separated and makes the code more testable.

### Question 3: How should FileWatcher integrate with fs_loop?

**Context**: `fs_loop` currently has a polling loop with a TODO. We need to replace that with real file watching.

**Options**:
- A) Pass `FileWatcher` as a parameter to `fs_loop`
- B) Create `FileWatcher` inside `fs_loop`
- C) Use a channel-based approach where watcher sends events to `fs_loop`

**Suggested Answer**: Option B - Create `FileWatcher` inside `fs_loop`. This keeps the interface simple and encapsulates the watcher lifecycle. The watcher can use an internal channel and `fs_loop` can poll it with `tokio::select!`.

### Question 4: How should we handle path normalization?

**Context**: OS file events give absolute paths, but `LpFs` uses paths relative to project root with leading `/`.

**Suggested Answer**: Normalize paths in `FileWatcher`:
- Convert absolute paths to relative (strip project root)
- Add leading `/` to match `LpFs` conventions
- Handle both Unix and Windows path separators

### Question 5: Should we watch directories or only files?

**Context**: `sync_file_change` only handles files. Directory creation/deletion might be needed for structure.

**Suggested Answer**: Watch directories but only sync files. When a directory is created, we don't need to sync it (directories are created implicitly when files are written). When a directory is deleted, we could delete all files in it, but for now, just skip directory events and only process file events.

### Question 6: How should we test file watching?

**Context**: We need to test that file changes are detected and synced correctly.

**Options**:
- A) Unit tests with `TempDir` and real file operations
- B) Integration tests with mock server
- C) Both

**Suggested Answer**: Option C - Both. Unit tests verify `FileWatcher` detects changes correctly. Integration tests verify the full sync flow works end-to-end.

## Notes

- The `notify` crate supports async via `tokio` integration
- We should use `notify::RecommendedWatcher` for cross-platform support
- Need to handle edge cases: rapid changes, file moves/renames, symlinks
- Debouncing is already implemented and tested, just needs to be fed real events

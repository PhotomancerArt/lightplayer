# Phase 4: Implement In-Memory Filesystem with Change Tracking

## Goal

Implement an in-memory filesystem with change tracking for easy testing.

## Tasks

1. Create `lp-core/src/fs/memory.rs`:
   - `InMemoryFilesystem` struct with `HashMap<String, Vec<u8>>` for file storage
   - Implement `Filesystem` trait:
     - `read_file()` - read from HashMap
     - `write_file()` - write to HashMap, track changes
     - `file_exists()` - check HashMap
     - `list_dir()` - scan HashMap keys for matching paths
   - Change tracking:
     - `changes: Vec<FileChange>` field
     - Track creates, modifies, deletes
     - `get_changes() -> Vec<FileChange>` - return and clear changes
     - `reset_changes()` - clear without returning

2. Implement path validation:
   - Ensure paths are relative to project root
   - Validate path format (leading slash, etc.)

3. Add tests demonstrating the testing workflow:
   - Create filesystem
   - Write files
   - Get changes
   - Mutate filesystem
   - Validate changes tracked correctly

## Success Criteria

- `InMemoryFilesystem` implements `Filesystem` trait
- Change tracking works correctly
- `list_dir()` works for in-memory filesystem
- Tests pass
- Code compiles without warnings


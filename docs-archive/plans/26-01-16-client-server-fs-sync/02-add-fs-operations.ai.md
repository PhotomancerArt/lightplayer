# Phase 2: Add Delete Operations and Recursive Listing to LpFs Trait

## Goal

Extend `LpFs` trait with delete operations and recursive listing support. Implement in both `LpFsStd` and `LpFsMemory`.

## Tasks

1. Update `lp-shared/src/fs/lp_fs.rs`:
   - Add `fn delete_file(&self, path: &str) -> Result<(), FsError>` to trait
   - Add `fn delete_dir(&self, path: &str) -> Result<(), FsError>` to trait (always recursive)
   - Update `fn list_dir(&self, path: &str) -> Result<Vec<String>, FsError>` to `fn list_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>, FsError>`

2. Update `lp-shared/src/fs/lp_fs_std.rs`:
   - Implement `delete_file()`:
     - Use `resolve_and_validate()` to get full path
     - Explicitly reject "/" before any operation
     - Check if path is file (not directory)
     - Call `fs::remove_file()`
   - Implement `delete_dir()`:
     - Use `resolve_and_validate()` to get full path
     - Explicitly reject "/" before any operation
     - Check if path is directory
     - Call `fs::remove_dir_all()` (recursive)
   - Update `list_dir()`:
     - Add `recursive: bool` parameter
     - If `recursive == false`, use existing logic (immediate children only)
     - If `recursive == true`, walk directory tree recursively
   - Extract path validation logic to helper function for testing

3. Update `lp-shared/src/fs/lp_fs_mem.rs`:
   - `LpFsMemory` already has `delete_file()` method - add to trait implementation
   - Implement `delete_dir()`:
     - Find all files/directories with path prefix
     - Remove them all (recursive by nature)
   - Update `list_dir()`:
     - Add `recursive: bool` parameter
     - If `recursive == false`, use existing logic
     - If `recursive == true`, return all files/directories with path prefix

4. Add path validation helper function:
   - Extract logic for checking if path is safe to delete
   - Place in `lp-shared/src/fs/lp_fs_std.rs` (or shared module)
   - Test this function separately (don't test by attempting dangerous operations)

5. Update all callers of `list_dir()`:
   - Find all uses of `list_dir()` in codebase
   - Update to pass `recursive: false` (or appropriate value)

6. Add tests:
   - Test `delete_file()` and `delete_dir()` in `LpFsMemory`
   - Test `list_dir()` with `recursive: true` and `recursive: false`
   - Test path validation helper function
   - Test that deleting "/" is rejected
   - Test that deleting outside root is rejected

## Success Criteria

- `LpFs` trait has `delete_file()`, `delete_dir()`, and updated `list_dir(recursive: bool)`
- Both `LpFsStd` and `LpFsMemory` implement new methods
- Path validation prevents deleting "/" and paths outside root
- All existing callers of `list_dir()` updated
- Tests verify delete operations work correctly
- Tests verify path validation works correctly
- All code compiles without warnings

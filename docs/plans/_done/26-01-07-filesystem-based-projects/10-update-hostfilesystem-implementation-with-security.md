# Phase 10: Update HostFilesystem Implementation with Security

## Goal

Update `HostFilesystem` to add `list_dir()` and ensure security with root path validation.

## Tasks

1. Move `fw-host/src/filesystem.rs` to `fw-host/src/fs/host.rs`:
   - Update module structure

2. Update `HostFilesystem`:
   - Ensure `root_path: PathBuf` field exists
   - Add `list_dir()` implementation:
     - Use `std::fs::read_dir()` to list directory contents
     - Return paths relative to project root
   - Add path validation:
     - All paths must be resolved relative to `root_path`
     - Validate that resolved paths stay within `root_path` (security)
     - Reject paths that would escape the project directory

3. Update all `HostFilesystem` methods:
   - Ensure path validation in `read_file()`, `write_file()`, `file_exists()`
   - All paths resolved relative to `root_path`

4. Update `fw-host/src/main.rs`:
   - Update imports for new filesystem location
   - Ensure `HostFilesystem::new()` takes root path

5. Add tests:
   - Test path validation (reject paths outside root)
   - Test `list_dir()` works correctly
   - Test security (attempt to access files outside root fails)

## Success Criteria

- `HostFilesystem` implements `list_dir()`
- Path validation prevents access outside root directory
- All methods validate paths
- Tests pass
- Code compiles without warnings


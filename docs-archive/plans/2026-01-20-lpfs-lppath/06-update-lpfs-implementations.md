# Phase 6: Update LpFs implementations

## Description

Update all `LpFs` implementations to work with the new trait signatures. Convert `P: AsRef<LpPath>` to normalized strings via `LpPathBuf::from()`.

## Implementation

1. Update `lp-shared/src/fs/lp_fs_mem.rs`:
   - Update `impl LpFs for LpFsMemory`:
     - Convert `P: AsRef<LpPath>` to `LpPathBuf` for normalization: `let normalized = LpPathBuf::from(path.as_ref().as_str());`
     - Use `normalized.as_str()` for internal operations
     - Update `list_dir()` to return `Vec<LpPathBuf>` (convert strings to `LpPathBuf`)

2. Update `lp-shared/src/fs/lp_fs_std.rs`:
   - Update `impl LpFs for LpFsStd`:
     - Convert `P: AsRef<LpPath>` to `LpPathBuf` for normalization
     - Use normalized string for path resolution
     - Update `list_dir()` to return `Vec<LpPathBuf>`

3. Update `lp-shared/src/fs/lp_fs_view.rs`:
   - Update `impl LpFs for LpFsView`:
     - Convert `P: AsRef<LpPath>` to `LpPathBuf` for normalization
     - Use normalized string for path translation
     - Update `list_dir()` to return `Vec<LpPathBuf>`
     - Convert parent paths to `LpPathBuf` before returning

## Success Criteria

- All `LpFs` implementations updated
- Code compiles
- Normalization happens via `LpPathBuf::from()`
- `list_dir()` returns `Vec<LpPathBuf>`
- Tests pass

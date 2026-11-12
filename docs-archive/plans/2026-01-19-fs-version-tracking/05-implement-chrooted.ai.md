# Phase 5: Implement chrooted filesystem versioning

## Description

Implement version tracking for chrooted filesystems. They should query the parent filesystem for changes, filter by path prefix, and translate to chrooted-relative paths.

## Implementation

1. Update `lp-shared/src/fs/lp_fs_mem.rs`:
   - Store parent filesystem reference in `ChrootedLpFsMemory` struct
   - Update `chroot()` to store parent reference
   - Implement `current_version()` - delegates to parent
   - Implement `get_changes_since()` - queries parent, filters by prefix, translates paths
   - Implement `clear_changes_before()` - no-op (parent manages versions)
   - Implement `record_changes()` - no-op (parent manages versions)

2. Update `lp-shared/src/fs/lp_fs_std.rs`:
   - Similar implementation for chrooted `LpFsStd` if it exists

**Note**: This may require changing the `chroot()` return type or storing parent references. The current implementation creates a new filesystem with copied files, so we may need to store a reference to the parent.

## Success Criteria

- Chrooted filesystems query parent for changes
- Path filtering works correctly (only changes within chroot scope)
- Path translation works correctly (parent paths → chrooted-relative paths)
- Code compiles without errors
- Tests pass

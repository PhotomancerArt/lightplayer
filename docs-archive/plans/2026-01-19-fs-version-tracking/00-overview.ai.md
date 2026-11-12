# Plan: Filesystem Version-Based Change Tracking

## Overview

Implement version-based change tracking for filesystems, similar to the frame-based versioning used for nodes. This enables immutable queries, multiple independent consumers, better deletion handling, and alignment with existing versioning patterns.

## Phases

1. Create FsVersion newtype
2. Add versioning methods to LpFs trait
3. Implement version tracking in LpFsMemory
4. Implement version tracking in LpFsStd
5. Implement chrooted filesystem versioning
6. Add last_fs_version to Project struct
7. Integrate version-based change tracking into server tick loop
8. Update fs_loop to record changes in LpFsStd
9. Cleanup and finalization

## Success Criteria

- `FsVersion` newtype created and used throughout
- `LpFs` trait extended with versioning methods (all required, no optional)
- `LpFsMemory` tracks versions from its own operations
- `LpFsStd` tracks versions from `record_changes()` calls
- Chrooted filesystems query parent and filter/translate changes
- Server queries changes in `tick()` and notifies projects
- Projects store and update `last_fs_version`
- All code compiles without errors
- Tests pass
- File changes are properly propagated to projects

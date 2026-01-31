# Phase 6: Add last_fs_version to Project struct

## Description

Add `last_fs_version` field to `Project` struct to track the last filesystem version processed by the project.

## Implementation

1. Update `lp-server/src/project.rs`:
   - Add `last_fs_version: FsVersion` field to `Project` struct
   - Initialize to `FsVersion::default()` in `new()`
   - Add `last_fs_version(&self) -> FsVersion` getter method
   - Add `update_fs_version(&mut self, version: FsVersion)` setter method

2. Import `FsVersion` from `lp_shared::fs`

## Success Criteria

- `Project` struct has `last_fs_version` field
- Getter and setter methods work correctly
- Initialized to default version (0)
- Code compiles without errors

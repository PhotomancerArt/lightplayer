# Phase 2: Add versioning methods to LpFs trait

## Description

Add versioning methods to the `LpFs` trait. All methods are required (no default implementations).

## Implementation

1. Update `lp-shared/src/fs/lp_fs.rs`:
   - Add `current_version(&self) -> FsVersion`
   - Add `get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange>`
   - Add `clear_changes_before(&mut self, before_version: FsVersion)`
   - Add `record_changes(&mut self, changes: Vec<FsChange>)`
   - All methods are required (no default impls)

2. Update trait documentation to explain versioning behavior

## Success Criteria

- `LpFs` trait has all four versioning methods
- Methods are required (no default implementations)
- Documentation is clear
- Code compiles (implementations will be added in later phases)

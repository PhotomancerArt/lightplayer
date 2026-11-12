# Phase 1: Create FsVersion newtype

## Description

Create the `FsVersion` newtype wrapper around `i64` to represent filesystem versions. This should follow the same pattern as `FrameId`.

## Implementation

1. Add `FsVersion` to `lp-shared/src/fs/fs_event.rs`:
   - Newtype wrapper around `i64`
   - Implement `new()`, `as_i64()`, `next()` methods
   - Implement `Default` (returns `FsVersion(0)`)
   - Implement `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`

2. Export `FsVersion` from `lp-shared/src/fs/mod.rs`

## Success Criteria

- `FsVersion` newtype created
- Methods match `FrameId` pattern
- Exported from `fs` module
- Code compiles without errors

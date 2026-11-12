# Plan: Local Filesystem Detection and Sync

## Overview

This plan implements real-time filesystem watching and syncing for the `dev` command. Currently, `fs_loop` has a TODO and doesn't actually detect file changes - it just polls. We need to:

1. Add filesystem watching capabilities using a cross-platform library
2. Integrate the watcher into `fs_loop` to detect changes
3. Ensure changes are properly synced to the server
4. Add tests to verify the functionality

## Current State

- `fs_loop` exists but only has polling logic (TODO comment on line 50)
- `sync_file_change` function exists and works correctly
- `LpFsStd` provides filesystem access but no watching
- `LpFsMemory` has change tracking for testing, but that's not suitable for real filesystem watching
- Debouncing logic exists and is tested

## Goals

1. **File Watching**: Detect file changes (create, modify, delete) in the project directory
2. **Change Detection**: Convert OS-level file events into `FsChange` events
3. **Integration**: Feed detected changes into the existing `fs_loop` debouncing and sync logic
4. **Testing**: Add tests to verify file watching and syncing work correctly

## Non-Goals

- Watching files outside the project directory (security boundary)
- Syncing from server to local (that's pull, not push)
- Handling symlinks or special files (for now)

## Success Criteria

- Running `lp-cli dev test-project` detects file changes in real-time
- Changes are debounced and synced to the server within 500ms
- Create, modify, and delete operations all work correctly
- Tests verify the functionality works

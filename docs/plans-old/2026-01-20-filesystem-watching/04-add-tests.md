# Phase 4: Add Tests

## Goal

Add comprehensive tests for file watching and syncing functionality.

## Test Categories

### 1. FileWatcher Unit Tests

**File**: `lp-app/apps/lp-cli/src/commands/dev/watcher.rs` (test module)

**Tests**:
- `test_filewatcher_creation()` - Verify watcher can be created
- `test_filewatcher_detects_create()` - Create a file, verify event
- `test_filewatcher_detects_modify()` - Modify a file, verify event
- `test_filewatcher_detects_delete()` - Delete a file, verify event
- `test_filewatcher_path_normalization()` - Verify paths are normalized correctly
- `test_filewatcher_skips_directories()` - Verify directory events are skipped

### 2. fs_loop Integration Tests

**File**: `lp-app/apps/lp-cli/tests/file_watch_sync.rs` (extend existing)

**Tests**:
- `test_fs_loop_detects_and_syncs_changes()` - End-to-end test with real filesystem
- `test_fs_loop_debouncing()` - Verify debouncing works with watcher
- `test_fs_loop_multiple_changes()` - Multiple rapid changes are batched

### 3. Manual Testing Checklist

- [ ] Create file → synced to server
- [ ] Modify file → synced to server
- [ ] Delete file → deleted on server
- [ ] Rapid changes → debounced correctly
- [ ] Nested directory changes → handled correctly
- [ ] Large files → synced correctly

## Test Implementation Details

**Using TempDir**:
```rust
use tempfile::TempDir;

#[tokio::test]
async fn test_filewatcher_detects_create() {
    let temp_dir = TempDir::new().unwrap();
    let mut watcher = FileWatcher::new(temp_dir.path().to_path_buf()).unwrap();
    
    // Create a file
    std::fs::write(temp_dir.path().join("test.txt"), b"content").unwrap();
    
    // Wait for event (with timeout)
    let change = tokio::time::timeout(
        Duration::from_secs(1),
        watcher.next_change()
    ).await.unwrap();
    
    assert!(change.is_some());
    assert_eq!(change.unwrap().path, "/test.txt");
}
```

**Mock Server**:
- Use `LocalServerTransport` for integration tests
- Verify files are written to server filesystem
- Check that paths are correct

## Verification

- [ ] All unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing confirms functionality

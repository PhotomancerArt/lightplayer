# Design: Filesystem Watching and Sync

## Architecture

```
┌─────────────────┐
│   File System   │
│   (OS Events)   │
└────────┬────────┘
         │
         │ file events
         ▼
┌─────────────────┐
│  FileWatcher    │  ← New module
│  (notify crate) │
└────────┬────────┘
         │
         │ FsChange events
         │ (via channel)
         ▼
┌─────────────────┐
│    fs_loop      │  ← Existing
│  (debouncing)   │
└────────┬────────┘
         │
         │ synced changes
         ▼
┌─────────────────┐
│ sync_file_change│  ← Existing
│   (to server)   │
└─────────────────┘
```

## Components

### 1. FileWatcher Module (`watcher.rs`)

**Purpose**: Wrap a cross-platform file watching library and convert OS events to `FsChange` events.

**Dependencies**: 
- `notify` crate (cross-platform file watching)
- `tokio` for async channels

**Key Types**:
```rust
pub struct FileWatcher {
    watcher: notify::RecommendedWatcher,
    event_receiver: tokio::sync::mpsc::Receiver<FsChange>,
    root_path: PathBuf,
}

impl FileWatcher {
    pub fn new(root_path: PathBuf) -> Result<Self>;
    pub async fn next_change(&mut self) -> Option<FsChange>;
}
```

**Event Mapping**:
- `notify::EventKind::Create` → `ChangeType::Create`
- `notify::EventKind::Modify` → `ChangeType::Modify`
- `notify::EventKind::Remove` → `ChangeType::Delete`

**Path Normalization**:
- Convert absolute paths to relative paths (from project root)
- Normalize paths to match `LpFs` conventions (leading `/`)

### 2. Updated fs_loop

**Changes**:
- Replace TODO polling with `FileWatcher::next_change()` calls
- Keep existing debouncing logic
- Keep existing sync logic

**Flow**:
```rust
loop {
    // Wait for file change (non-blocking with timeout)
    if let Some(change) = watcher.next_change().await {
        add_pending_change(&mut pending_changes, &mut last_change_time, change);
    }
    
    // Check debounce and sync (existing logic)
    if should_sync {
        // ... existing sync logic ...
    }
    
    // Small sleep to avoid busy-waiting
    sleep(Duration::from_millis(10)).await;
}
```

### 3. Integration Points

**handler.rs**:
- Create `FileWatcher` in `handle_dev_async`
- Pass watcher to `fs_loop` (or create it inside `fs_loop`)

**fs_loop.rs**:
- Accept `FileWatcher` or create it internally
- Use watcher instead of polling

## Error Handling

- **Watcher creation failure**: Return error, don't start dev command
- **Watcher errors during runtime**: Log warning, continue watching
- **Path normalization failures**: Skip the event, log warning
- **Sync failures**: Log error, continue watching (existing behavior)

## Edge Cases

1. **Rapid file changes**: Debouncing handles this
2. **Directory creation**: Watch for directory events, but only sync files
3. **File moves/renames**: May generate Create + Delete, handle gracefully
4. **Project directory changes**: Only watch within project root
5. **Symlinks**: Skip for now (notify handles this)

## Testing Strategy

1. **Unit tests**: Test `FileWatcher` event conversion
2. **Integration tests**: Test `fs_loop` with real file changes
3. **Manual testing**: Verify with actual project directory

## Dependencies

Add to `lp-cli/Cargo.toml`:
```toml
[dependencies]
notify = "6.1"  # Cross-platform file watching
```

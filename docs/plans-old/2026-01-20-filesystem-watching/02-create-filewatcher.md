# Phase 2: Create FileWatcher Module

## Goal

Create a `FileWatcher` module that wraps `notify` and converts OS events to `FsChange` events.

## Steps

1. **Create watcher.rs module**
   - File: `lp-app/apps/lp-cli/src/commands/dev/watcher.rs`
   - Export from `mod.rs`

2. **Implement FileWatcher struct**
   ```rust
   pub struct FileWatcher {
       watcher: notify::RecommendedWatcher,
       event_receiver: tokio::sync::mpsc::Receiver<FsChange>,
       root_path: PathBuf,
   }
   ```

3. **Implement new()**
   - Create `notify::RecommendedWatcher` with async handler
   - Create `tokio::sync::mpsc::channel` for events
   - Watch the project root directory recursively
   - Return `FileWatcher` instance

4. **Implement event handler**
   - Convert `notify::Event` to `FsChange`
   - Normalize paths (absolute → relative from root)
   - Filter out directory-only events (we only sync files)
   - Send events to channel

5. **Implement next_change()**
   - Non-blocking receive from channel
   - Return `Option<FsChange>`

## Key Implementation Details

**Path Normalization**:
```rust
fn normalize_path(absolute_path: &Path, root: &Path) -> String {
    // Get relative path from root
    let relative = absolute_path.strip_prefix(root).unwrap();
    // Convert to string with leading "/"
    format!("/{}", relative.to_string_lossy().replace('\\', "/"))
}
```

**Event Filtering**:
- Skip events for directories (check `event.paths[].is_dir()`)
- Skip events outside project root
- Handle `Create`, `Modify`, `Remove` event kinds

**Error Handling**:
- Log watcher errors but continue
- Return `None` from `next_change()` if channel is closed

## Verification

- [ ] Module compiles
- [ ] Can create `FileWatcher` for a directory
- [ ] Events are received and converted correctly

## Testing

Create basic test:
```rust
#[tokio::test]
async fn test_filewatcher_creation() {
    let temp_dir = TempDir::new().unwrap();
    let watcher = FileWatcher::new(temp_dir.path().to_path_buf()).unwrap();
    // Verify watcher was created
}
```

# Phase 3: Integrate FileWatcher into fs_loop

## Goal

Replace the TODO polling logic in `fs_loop` with real file watching using `FileWatcher`.

## Steps

1. **Update fs_loop signature**
   - Add `FileWatcher` parameter (or create it inside)
   - Option: Create watcher inside `fs_loop` to keep interface simple

2. **Replace polling logic**
   - Remove TODO comment
   - Replace sleep loop with watcher integration
   - Use `watcher.next_change().await` to get events

3. **Update handler.rs**
   - Create `FileWatcher` before spawning `fs_loop`
   - Pass watcher to `fs_loop` (or let it create its own)

## Implementation

**Option A: Pass watcher as parameter**
```rust
pub async fn fs_loop(
    transport: Arc<tokio::sync::Mutex<Box<dyn ClientTransport>>>,
    project_dir: PathBuf,
    project_uid: String,
    local_fs: Arc<dyn LpFs + Send + Sync>,
    mut watcher: FileWatcher,
) -> Result<()>
```

**Option B: Create watcher inside fs_loop** (preferred)
```rust
pub async fn fs_loop(
    transport: Arc<tokio::sync::Mutex<Box<dyn ClientTransport>>>,
    project_dir: PathBuf,
    project_uid: String,
    local_fs: Arc<dyn LpFs + Send + Sync>,
) -> Result<()> {
    // Create watcher
    let mut watcher = FileWatcher::new(project_dir.clone())
        .context("Failed to create file watcher")?;
    
    // ... rest of loop ...
}
```

**Updated loop logic**:
```rust
loop {
    // Wait for file change with timeout (non-blocking)
    tokio::select! {
        change = watcher.next_change() => {
            if let Some(change) = change {
                add_pending_change(&mut pending_changes, &mut last_change_time, change);
            }
        }
        _ = sleep(Duration::from_millis(50)) => {
            // Timeout - check debounce
        }
    }
    
    // Check if debounce period has passed
    let should_sync = if let Some(last_time) = last_change_time {
        last_time.elapsed() >= DEBOUNCE_DURATION && !pending_changes.is_empty()
    } else {
        false
    };
    
    if should_sync {
        // ... existing sync logic ...
    }
}
```

## Error Handling

- **Watcher creation failure**: Return error from `fs_loop`
- **Watcher errors during runtime**: Log warning, continue (watcher handles this internally)
- **Channel closed**: Exit loop gracefully

## Verification

- [ ] `fs_loop` compiles with watcher integration
- [ ] File changes trigger sync (manual testing)
- [ ] Debouncing still works correctly

## Testing

Update existing tests or add new ones:
- Test that file changes are detected
- Test that debouncing works
- Test that sync is called with correct changes

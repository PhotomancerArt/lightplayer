# Design: Filesystem Version-Based Change Tracking

## Overview

Implement version-based change tracking for filesystems, similar to the frame-based versioning used for nodes. This enables immutable queries, multiple independent consumers, better deletion handling, and alignment with existing versioning patterns.

## Goals

1. Add `FsVersion` newtype (`i64`) for filesystem versioning
2. Extend `LpFs` trait with versioning methods (all implementations required)
3. Implement version tracking in `LpFsMemory` (tracks from own operations)
4. Implement version tracking in `LpFsStd` (accepts external changes via `record_changes()`)
5. Implement chrooted filesystem versioning (queries parent, filters, translates paths)
6. Integrate version-based change tracking into server tick loop
7. Store last processed version per project in `Project` struct

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        LpServer                              │
│                                                               │
│  ┌──────────────┐         ┌──────────────────┐              │
│  │   base_fs    │────────▶│ ProjectManager   │              │
│  │ (LpFsMemory)│         │                  │              │
│  │              │         │  ┌────────────┐  │              │
│  │ Version: 42  │         │  │ Project   │  │              │
│  │ Changes:     │         │  │ last_ver:  │  │              │
│  │ /path1 (40)  │         │  │    38      │  │              │
│  │ /path2 (42)  │         │  └────────────┘  │              │
│  └──────────────┘         └──────────────────┘              │
│         │                                                    │
│         │ tick() queries:                                   │
│         │ get_changes_since(38)                             │
│         │ → returns changes for /path1, /path2              │
│         │                                                    │
│         │ Filters by project path prefix                    │
│         │ Translates to project-relative paths             │
│         │ Calls project.runtime().handle_fs_changes()       │
└─────────┼──────────────────────────────────────────────────┘
          │
          │
┌─────────┼──────────────────────────────────────────────────┐
│         │         Client-side (lp-cli)                      │
│         │                                                    │
│  ┌──────▼──────┐         ┌──────────────┐                   │
│  │ FileWatcher │────────▶│   LpFsStd    │                   │
│  │  (notify)   │         │              │                   │
│  │             │         │ record_changes│                  │
│  │ FsChange    │         │              │                   │
│  │ events      │         │ Version: 15  │                   │
│  └─────────────┘         └──────────────┘                   │
│                                                               │
│  Changes synced to server via fs_write/fs_delete_file        │
└──────────────────────────────────────────────────────────────┘
```

## File Structure

```
lp-app/crates/lp-shared/src/fs/
├── mod.rs                          # UPDATE: Export FsVersion
├── fs_event.rs                     # UPDATE: Add FsVersion newtype
├── lp_fs.rs                        # UPDATE: Add versioning trait methods
├── lp_fs_mem.rs                    # UPDATE: Implement version tracking
└── lp_fs_std.rs                    # UPDATE: Implement version tracking

lp-app/crates/lp-server/src/
├── server.rs                       # UPDATE: Query changes in tick()
├── project.rs                      # UPDATE: Add last_fs_version field
└── project_manager.rs              # UPDATE: Initialize last_fs_version

lp-app/apps/lp-cli/src/commands/dev/
└── fs_loop.rs                      # UPDATE: Call record_changes() on LpFsStd
```

## Types and Functions

### fs_event.rs (UPDATE)

```rust
/// Filesystem version identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FsVersion(pub i64);

impl FsVersion {
    pub fn new(id: i64) -> Self;
    pub fn as_i64(self) -> i64;
    pub fn next(self) -> Self;
}

impl Default for FsVersion {
    fn default() -> Self {
        Self(0)
    }
}
```

### lp_fs.rs (UPDATE)

```rust
pub trait LpFs {
    // ... existing methods ...
    
    /// Get the current filesystem version
    ///
    /// Returns the version number that will be assigned to the next change.
    /// If no changes have occurred, returns the initial version (typically 0).
    fn current_version(&self) -> FsVersion;
    
    /// Get all changes since a specific version
    ///
    /// Returns changes for paths that were modified at or after `since_version`.
    /// Changes are returned with paths relative to the filesystem root.
    /// Only the latest change per path is returned (if a file was modified
    /// multiple times, only the most recent change is included).
    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange>;
    
    /// Clear changes older than the specified version
    ///
    /// Removes change tracking for versions older than `before_version`.
    /// This is useful for memory management when no consumers need old versions.
    fn clear_changes_before(&mut self, before_version: FsVersion);
    
    /// Record externally detected changes
    ///
    /// Used by filesystem implementations that don't directly track changes
    /// (e.g., `LpFsStd` receiving changes from `FileWatcher`).
    /// Each change is assigned the next version number.
    fn record_changes(&mut self, changes: Vec<FsChange>);
}
```

### lp_fs_mem.rs (UPDATE)

```rust
pub struct LpFsMemory {
    files: RefCell<HashMap<String, Vec<u8>>>,
    /// Version counter (increments on each change)
    current_version: RefCell<FsVersion>,
    /// Map of path -> (version, ChangeType) - only latest change per path
    changes: RefCell<HashMap<String, (FsVersion, ChangeType)>>,
}

impl LpFsMemory {
    // ... existing methods ...
    
    fn record_change(&self, path: String, change_type: ChangeType) {
        let mut current = self.current_version.borrow_mut();
        *current = current.next();
        let version = *current;
        drop(current);
        
        self.changes.borrow_mut().insert(path, (version, change_type));
    }
}

impl LpFs for LpFsMemory {
    // ... existing implementations ...
    
    fn current_version(&self) -> FsVersion {
        *self.current_version.borrow()
    }
    
    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        self.changes
            .borrow()
            .iter()
            .filter_map(|(path, (version, change_type))| {
                if *version >= since_version {
                    Some(FsChange {
                        path: path.clone(),
                        change_type: *change_type,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
    
    fn clear_changes_before(&mut self, before_version: FsVersion) {
        self.changes.borrow_mut().retain(|_, (version, _)| {
            *version >= before_version
        });
    }
    
    fn record_changes(&mut self, changes: Vec<FsChange>) {
        for change in changes {
            self.record_change(change.path, change.change_type);
        }
    }
}
```

### lp_fs_std.rs (UPDATE)

```rust
pub struct LpFsStd {
    root_path: PathBuf,
    /// Version counter (increments on each change)
    current_version: RefCell<FsVersion>,
    /// Map of path -> (version, ChangeType) - only latest change per path
    changes: RefCell<HashMap<String, (FsVersion, ChangeType)>>,
}

impl LpFsStd {
    // ... existing methods ...
    
    fn record_change(&self, path: String, change_type: ChangeType) {
        let mut current = self.current_version.borrow_mut();
        *current = current.next();
        let version = *current;
        drop(current);
        
        self.changes.borrow_mut().insert(path, (version, change_type));
    }
}

impl LpFs for LpFsStd {
    // ... existing implementations ...
    
    fn current_version(&self) -> FsVersion {
        *self.current_version.borrow()
    }
    
    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        self.changes
            .borrow()
            .iter()
            .filter_map(|(path, (version, change_type))| {
                if *version >= since_version {
                    Some(FsChange {
                        path: path.clone(),
                        change_type: *change_type,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
    
    fn clear_changes_before(&mut self, before_version: FsVersion) {
        self.changes.borrow_mut().retain(|_, (version, _)| {
            *version >= before_version
        });
    }
    
    fn record_changes(&mut self, changes: Vec<FsChange>) {
        for change in changes {
            // Normalize path to match LpFs conventions
            let normalized = Self::normalize_path(&change.path);
            self.record_change(normalized, change.change_type);
        }
    }
}
```

### Chrooted Filesystems (UPDATE)

```rust
// In lp_fs_mem.rs - ChrootedLpFsMemory
impl LpFs for ChrootedLpFsMemory {
    // ... existing implementations ...
    
    fn current_version(&self) -> FsVersion {
        // Query parent's version (would need to store parent reference)
        // For now, return parent's version
        FsVersion::default() // TODO: Store parent reference
    }
    
    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        // Query parent's changes, filter by path prefix, translate paths
        // Implementation depends on storing parent reference
        Vec::new() // TODO: Implement
    }
    
    fn clear_changes_before(&mut self, _before_version: FsVersion) {
        // No-op for chrooted filesystems (parent manages versions)
    }
    
    fn record_changes(&mut self, _changes: Vec<FsChange>) {
        // No-op for chrooted filesystems (parent manages versions)
    }
}
```

**Note**: Chrooted filesystems need to store a reference to the parent filesystem to query changes. This may require changes to the `chroot()` return type or storing parent references.

### project.rs (UPDATE)

```rust
pub struct Project {
    name: String,
    path: String,
    runtime: ProjectRuntime,
    /// Last filesystem version processed by this project
    last_fs_version: FsVersion,
}

impl Project {
    pub fn new(...) -> Result<Self, ServerError> {
        // ...
        Ok(Self {
            name,
            path,
            runtime,
            last_fs_version: FsVersion::default(),
        })
    }
    
    pub fn last_fs_version(&self) -> FsVersion {
        self.last_fs_version
    }
    
    pub fn update_fs_version(&mut self, version: FsVersion) {
        self.last_fs_version = version;
    }
}
```

### server.rs (UPDATE)

```rust
impl LpServer {
    pub fn tick(&mut self, delta_ms: u32, incoming: Vec<Message>) -> Result<Vec<Message>, ServerError> {
        // ... process incoming messages ...
        
        // Query filesystem changes from base_fs
        let project_handles: Vec<_> = self
            .project_manager
            .list_loaded_projects()
            .iter()
            .map(|p| p.handle)
            .collect();
        
        // Get changes for each project
        for handle in &project_handles {
            if let Some(project) = self.project_manager.get_project_mut(*handle) {
                let last_version = project.last_fs_version();
                let project_path = project.path();
                
                // Query changes from base_fs
                let base_changes = self.base_fs().get_changes_since(last_version);
                
                // Filter changes for this project
                let project_prefix = format!("{}/", project_path);
                let project_changes: Vec<FsChange> = base_changes
                    .into_iter()
                    .filter_map(|change| {
                        if change.path.starts_with(&project_prefix) {
                            // Translate to project-relative path
                            let relative_path = &change.path[project_prefix.len()..];
                            let normalized = if relative_path.starts_with('/') {
                                relative_path.to_string()
                            } else {
                                format!("/{}", relative_path)
                            };
                            Some(FsChange {
                                path: normalized,
                                change_type: change.change_type,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                
                // Update project if there are changes
                if !project_changes.is_empty() {
                    project.runtime_mut().handle_fs_changes(&project_changes)
                        .map_err(|e| ServerError::Core(format!("Failed to handle fs changes: {}", e)))?;
                    
                    // Update last processed version
                    let current_version = self.base_fs().current_version();
                    project.update_fs_version(current_version);
                }
            }
        }
        
        // Tick all loaded projects
        // ... existing tick logic ...
        
        Ok(responses)
    }
}
```

### fs_loop.rs (UPDATE)

```rust
pub async fn fs_loop(
    transport: Arc<tokio::sync::Mutex<Box<dyn crate::client::transport::ClientTransport>>>,
    project_dir: PathBuf,
    project_uid: String,
    local_fs: Arc<dyn LpFs + Send + Sync>,
) -> Result<()> {
    // ... existing setup ...
    
    // Main loop
    loop {
        tokio::select! {
            change = watcher.next_change() => {
                if let Some(change) = change {
                    add_pending_change(&mut pending_changes, &mut last_change_time, change);
                } else {
                    break;
                }
            }
            _ = sleep(Duration::from_millis(50)) => {}
        }
        
        // ... debounce checking ...
        
        if should_sync {
            let changes: Vec<FsChange> = pending_changes.values().cloned().collect();
            pending_changes.clear();
            last_change_time = None;
            
            // Record changes in local_fs (if it's LpFsStd)
            if let Some(fs_std) = local_fs.as_any().downcast_ref::<LpFsStd>() {
                // Need mutable access - may require Arc<Mutex<LpFsStd>>
                // Or pass changes to a method that accepts them
            }
            
            // Sync to server
            for change in changes {
                sync_file_change(&client, &change, &project_uid, &project_dir, &local_fs).await?;
            }
        }
    }
    
    Ok(())
}
```

**Note**: Recording changes in `fs_loop` may require refactoring to use `Arc<Mutex<LpFsStd>>` or a different approach to get mutable access.

## Implementation Notes

1. **Version Initialization**: `FsVersion` starts at 0. First change gets version 1.

2. **Change Deduplication**: Only the latest change per path is stored. If a file is modified multiple times, only the most recent change is kept.

3. **Deletion Handling**: Deletions are naturally handled - if a file was deleted at version 5, querying since version 3 will show the delete, but querying since version 6 won't (the file is already gone).

4. **Chrooted Filesystem Parent Reference**: Chrooted filesystems need access to their parent to query changes. This may require:
   - Storing `Rc<RefCell<dyn LpFs>>` reference to parent in chrooted struct
   - Or changing `chroot()` to return a type that includes parent reference
   - Or using a different approach (e.g., server handles filtering)

5. **Memory Management**: Old versions can be cleared when no project needs them. The server can track the minimum `last_fs_version` across all projects and clear older versions periodically.

6. **LpFsStd Change Recording**: `fs_loop` needs to record changes in `LpFsStd`. Options:
   - Use `Arc<Mutex<LpFsStd>>` for mutable access
   - Or have `fs_loop` maintain a separate change tracker and merge periodically
   - Or refactor to pass changes through a different mechanism

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

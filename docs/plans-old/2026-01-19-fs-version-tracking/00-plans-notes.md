# Plan Notes: Filesystem Version-Based Change Tracking

## Context

Currently, `LpFsMemory` tracks filesystem changes using a `Vec<FsChange>` that gets cleared with `reset_changes()`. This approach:
- Requires mutable access to query/reset changes
- Consumes changes when reset, preventing multiple consumers
- Doesn't align well with the frame-based versioning used in `ProjectRuntime`
- Makes deletion tracking more complex (need to track deletions separately)

**Current State**:
- `LpFsMemory` has `get_changes()` and `reset_changes()` methods
- Changes are stored as `Vec<FsChange>` and cleared after consumption
- `ProjectRuntime` uses `FrameId` for versioning nodes
- Server needs to notify projects about filesystem changes, but currently doesn't

**Goal**: Implement version-based change tracking for filesystems, similar to the frame-based versioning used for nodes. This will enable:
- Immutable queries (no `&mut` needed to check changes)
- Multiple consumers querying independently
- Better deletion handling (deletions naturally handled by version comparison)
- Alignment with frame-based versioning patterns
- Memory-efficient cleanup of old versions

## Questions

### Question 1: What should the version type be?

**Context**: We need a newtype for filesystem versions. `FrameId` is `i64` and used for frame-based versioning. Filesystem versions are independent of frames.

**Options**:
- A) Use `u64` for `FsVersion` (simpler, independent of frames)
- B) Use `i64` to match `FrameId` (allows potential alignment later)
- C) Use `FrameId` directly (couples filesystem to frame system)

**Answer**: Option B - Use `i64` to match `FrameId`. This allows potential alignment later and keeps the types consistent.

### Question 2: Should all `LpFs` implementations support versioning?

**Context**: 
- `LpFsMemory` can track versions from its own `write_file`/`delete_file` operations
- `LpFsStd` (real filesystem) is used in `lp-cli` where `FileWatcher` already detects changes
- Currently, `FileWatcher` events are synced to server but `LpFsStd` doesn't track them
- The server's `base_fs` (could be `LpFsStd` for disk-based servers) needs to track changes

**Architecture Consideration**: 
- `FileWatcher` in `lp-cli` produces `FsChange` events from OS file watching
- These could be fed into `LpFsStd` to update its version tracking
- `LpFsStd` could have a method like `record_changes(&mut self, changes: Vec<FsChange>)` 
- This would make `LpFsStd` version-aware by accepting externally detected changes
- Both `LpFsMemory` and `LpFsStd` would track versions, but `LpFsMemory` tracks from its own ops, while `LpFsStd` tracks from `FileWatcher` events

**Options**:
- A) Require all implementations to support versioning - `LpFsMemory` tracks from ops, `LpFsStd` accepts `record_changes()` calls
- B) Make versioning optional with default impls (returns empty/version 0)
- C) Only `LpFsMemory` supports versioning, others return empty

**Answer**: Option A - Require all implementations to support versioning. `LpFsMemory` tracks changes from its own operations. `LpFsStd` accepts `record_changes()` calls to update version tracking from external sources (like `FileWatcher`). This makes the architecture consistent and allows both implementations to be version-aware.

### Question 3: How should chrooted filesystems handle versioning?

**Context**: Chrooted filesystems wrap a parent filesystem. They need to query the parent for changes and filter/translate paths.

**Options**:
- A) Chrooted filesystems query parent and filter changes
- B) Chrooted filesystems maintain their own version counter
- C) Chrooted filesystems delegate to parent and translate paths

**Answer**: Option A - Chrooted filesystems query parent's changes, filter by path prefix, and translate to chrooted-relative paths. They don't maintain their own version counter (they use the parent's versioning).

### Question 4: Where should we track last processed version per project?

**Context**: Projects need to know what version they last processed to query changes since then.

**Options**:
- A) Store in `ProjectManager` (centralized)
- B) Store in `Project` struct (per-project)
- C) Store in `ProjectRuntime` (engine-level)

**Answer**: Option B - Store in `Project` struct. The server will query changes from `base_fs` and pass them into the project. This keeps the version tracking close to the project data and allows each project to track its own state independently.

### Question 5: When should versions be cleared?

**Context**: Old versions consume memory. We need a way to clear versions older than a certain threshold.

**Options**:
- A) Manual call to `clear_changes_before()` when needed
- B) Automatic cleanup based on oldest active version
- C) Both - manual with automatic fallback

**Answer**: Option A - Manual call to `clear_changes_before()`. The server can call this periodically based on the oldest version any project is tracking. This gives explicit control and avoids complex automatic cleanup logic.

### Question 6: How should the server integrate version-based change tracking?

**Context**: `LpServer::tick()` needs to query changes from `base_fs` and notify projects.

**Options**:
- A) Query changes in `tick()` after processing messages, filter per project, call `handle_fs_changes()`
- B) Projects query their own changes from their chrooted filesystem
- C) Hybrid - server queries base_fs, projects query their chrooted view

**Answer**: Option A - Query changes in `tick()` after processing messages. The server has access to `base_fs` and can efficiently query once, then filter and translate for each project. This centralizes the logic and ensures all projects are notified consistently.

### Question 7: What should the trait methods look like?

**Context**: We need to add versioning methods to `LpFs` trait.

**Answer**: Add these methods to `LpFs` trait (all required, no default implementations):
- `current_version(&self) -> FsVersion` - get current version (immutable)
- `get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange>` - query changes (immutable)
- `clear_changes_before(&mut self, before_version: FsVersion)` - clear old changes (mutable)
- `record_changes(&mut self, changes: Vec<FsChange>)` - record externally detected changes (for `LpFsStd`)

All implementations must provide these methods. `LpFsMemory` tracks changes from its own operations, while `LpFsStd` uses `record_changes()` to accept changes from external sources like `FileWatcher`.

## Notes

- Version-based tracking naturally handles deletions: if a file was deleted at version 5, querying since version 3 will show the delete, but querying since version 6 won't
- This aligns with the existing frame-based versioning pattern in `ProjectRuntime`
- Multiple consumers can query independently without interfering
- Memory can be managed by clearing old versions when no project needs them
- Chrooted filesystems will need to implement path filtering and translation logic

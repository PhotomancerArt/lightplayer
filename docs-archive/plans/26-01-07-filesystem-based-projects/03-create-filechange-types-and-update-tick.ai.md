# Phase 3: Create FileChange Types and Update LpApp::tick Signature

## Goal

Create file change tracking types and update `LpApp::tick()` to accept file changes.

## Tasks

1. Create `FileChange` types in `lp-core/src/app/mod.rs` or new `file_change.rs`:
   ```rust
   pub enum ChangeType {
       Create,
       Modify,
       Delete,
   }

   pub struct FileChange {
       pub path: String,  // Path relative to project root
       pub change_type: ChangeType,
   }
   ```

2. Update `LpApp::tick()` signature in `lp-core/src/app/lp_app.rs`:
   ```rust
   pub fn tick(
       &mut self,
       delta_ms: u32,
       incoming: &[MsgIn],
       file_changes: &[FileChange],  // NEW
   ) -> Result<Vec<MsgOut>, Error>
   ```

3. Add placeholder handling for file changes (will be implemented in later phase):
   - Log file changes for now
   - Don't process them yet

4. Update `fw-host/src/main.rs`:
   - Pass empty `&[]` for `file_changes` parameter for now

## Success Criteria

- `FileChange` and `ChangeType` types exist
- `LpApp::tick()` signature updated
- All call sites updated (pass empty slice for now)
- Code compiles without warnings


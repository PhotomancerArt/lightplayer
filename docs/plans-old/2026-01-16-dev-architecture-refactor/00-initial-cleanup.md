# Phase 0: Initial Cleanup

## Description

Clean up the current state of the codebase by removing or commenting out old code that will be replaced, creating stub implementations to get things compiling, removing defunct tests, and ensuring the build is in a stable state with tests passing before starting implementation.

## Tasks

1. Check current state:
   - Identify all files/modules referenced but missing
   - Identify all code that will be replaced
   - Identify all tests that reference old code
   - Check what currently compiles and what doesn't

2. Update `lp-app/apps/lp-cli/src/commands/dev/mod.rs`:
   - Comment out references to non-existent modules:
     - `async_client` (will be recreated in `client/` directory in phase 5)
     - `handler` (will be recreated in phase 8)
     - `push` (will be recreated as `push_project.rs` in phase 6)
     - `sync` (check if exists, may be kept or recreated)
     - `watcher` (check if exists, may be kept or recreated)
   - Keep `args.rs` (still needed)
   - Comment out `pub use handler::handle_dev;` temporarily

3. Create stub `handler.rs`:
   - Create minimal `handle_dev()` function that:
     - Takes `DevArgs`
     - Returns `Result<()>`
     - Currently just returns `Ok(())` or prints "Not implemented"
   - This allows `main.rs` to compile
   - Add `// TODO: Will be reimplemented in phase 8` comment

4. Create stub `async_client.rs` (temporary, for compilation):
   - Create minimal `AsyncLpClient` struct
   - Create minimal `serializable_response_to_project_response` function
   - Add `#[allow(dead_code)]` attributes
   - Add `// TODO: Will be recreated in client/ directory in phase 5` comment
   - This allows `debug_ui/ui.rs` to compile

5. Update `lp-app/apps/lp-cli/src/main.rs`:
   - Ensure `dev::handle_dev` import works (should work with stub)
   - No changes needed if stub handler exists

6. Update `lp-app/apps/lp-cli/src/debug_ui/ui.rs`:
   - Should compile with stub `AsyncLpClient`
   - If not, comment out UI-related code temporarily
   - Add `// TODO: Will be updated in phase 8` comments

7. Create stub `sync.rs` (if referenced by tests):
   - Check if `sync_file_change` is referenced in tests
   - Create minimal stub function if needed
   - Add `#[allow(dead_code)]` and `// TODO: Will be recreated in phase 6` comment

8. Check for tests:
   - `lp-app/apps/lp-cli/tests/file_watch_sync.rs` - references `AsyncLpClient` and `sync_file_change`
   - `lp-app/apps/lp-cli/tests/integration.rs` - may reference removed code
   - Comment out tests that reference non-existent code
   - Add `#[ignore]` attribute to commented-out tests with note about phase
   - Ensure remaining tests compile and pass

9. Run build and tests:
   - Run `cargo check` - should compile without errors
   - Run `cargo test` - should pass (or skip commented-out tests)
   - Fix any compilation errors
   - Fix any test failures
   - Run `cargo +nightly fmt` to format code

10. Document what was removed/commented:
   - Add comments explaining what was removed and why
   - Note what will be recreated in later phases
   - Update this phase document with what was actually done

## Success Criteria

- Code compiles without errors (`cargo check` passes)
- All remaining tests pass (`cargo test` passes)
- No references to non-existent modules (or they're commented out)
- Stub implementations exist for required functions
- Clear comments explaining what was removed and why
- Codebase is in a stable state ready for implementation

## Implementation Notes

- Use `#[allow(dead_code)]` for stub functions if needed
- Use `todo!()` macro for stub implementations to make it clear they need implementation
- Add comments like `// TODO: Will be recreated in phase X` for removed code
- Keep `args.rs` as-is (still needed)
- Consider keeping `watcher.rs` if it's still useful (FileWatcher may still be needed)
- Consider keeping `sync.rs` if `sync_changes()` is still useful
- Be conservative - comment out rather than delete if unsure
- Can always remove commented code later once new implementation is working

# Phase 13: Cleanup and Testing

## Goal

Final cleanup, testing, and ensure everything works together.

## Tasks

1. Remove any temporary code, TODOs, debug prints

2. Fix all warnings:
   - Remove unused code (or mark with `#[allow(dead_code)]` if needed later)
   - Fix any clippy warnings

3. Ensure all tests pass:
   - Run existing tests
   - Add integration tests for full workflow:
     - Create project with in-memory FS
     - Load project
     - Mutate files
     - Get changes
     - Pass to tick()
     - Validate project state

4. Test real filesystem workflow:
   - Create project with `--create`
   - Edit files in IDE
   - Verify changes are detected and applied

5. Update documentation:
   - Ensure code comments are clear
   - Update any relevant README files

6. Format code:
   - Run `cargo +nightly fmt` on `lightplayer/` directory

7. Move plan to `_done`:
   - Move `lightplayer/plans/26-01-07-filesystem-based-projects/` to `lightplayer/plans/_done/26-01-07-filesystem-based-projects/`

## Success Criteria

- No temporary code or TODOs
- All warnings fixed
- All tests pass
- Code is clean and readable
- Code is formatted
- Plan moved to `_done`


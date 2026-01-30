# Phase 7: Test and Cleanup

## Tasks

1. Run filetests to verify output format:
   - Test compilation errors show correct format
   - Test execution errors show correct format
   - Test comparison failures show correct format
   - Test trap errors show correct format
   - Verify sections appear in correct order
   - Verify error details and rerun commands appear at the end
   - Test with `DEBUG=1` to verify debug sections appear
   - Test without `DEBUG=1` to verify debug sections are hidden
   - Verify scripts work correctly:
     - `scripts/glsl-filetests.sh matrix/mat4` (summary mode)
     - `scripts/glsl-filetests.sh matrix/mat4/op-add.glsl` (detail mode)

2. Verify file structure matches proposed structure:
   - All directories exist
   - All mod.rs files contain only re-exports
   - No old files remain

3. Fix any issues found

4. Remove any temporary code, TODOs, or debug prints

5. Fix all warnings

6. Ensure all tests pass

7. Verify that tailing the output shows error details and rerun commands at the end

8. Run `cargo +nightly fmt` on `lightplayer/` directory

9. Move plan directory to `lightplayer/plans/_done/`

## Success Criteria

- All tests pass
- Output format is consistent across all error types
- Sections appear in correct order
- Error details and rerun commands appear at the end
- No warnings
- Code is clean and readable
- File structure matches proposed structure
- All mod.rs files contain only re-exports
- Plan moved to `_done/` directory


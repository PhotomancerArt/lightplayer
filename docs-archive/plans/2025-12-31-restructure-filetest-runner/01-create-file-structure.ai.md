# Phase 1: Create New File Structure and OutputMode Enum

## Tasks

1. Create new file structure:
   - Create `output_mode.rs` with `OutputMode` enum
   - Create `colors.rs` (extract from lib.rs)
   - Create `discovery.rs` (extract test discovery logic)
   - Create `runner.rs` and `runner/concurrent.rs` (extract runner logic)
   - Create `parse/` directory structure
   - Create `test_run/` directory structure
   - Create `test_compile/` directory structure
   - Create `test_transform/` directory structure
   - Create `util/` directory structure

2. Define `OutputMode` enum in `output_mode.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum OutputMode {
       Summary,  // Minimal output, used for multiple tests
       Detail,   // Full output for single test
       Debug,    // Full output + debug sections (when DEBUG=1)
   }
   ```

3. Extract colors module from lib.rs to colors.rs

4. Create empty mod.rs files in all new directories (will be populated in later phases)

5. Update lib.rs to use new modules (stub implementations for now)

## Files to Create

- `src/output_mode.rs`
- `src/colors.rs`
- `src/discovery.rs`
- `src/runner.rs`
- `src/runner/mod.rs`
- `src/runner/concurrent.rs`
- `src/parse/mod.rs`
- `src/test_run/mod.rs`
- `src/test_compile/mod.rs`
- `src/test_transform/mod.rs`
- `src/util/mod.rs`

## Files to Modify

- `src/lib.rs` - Update to use new module structure

## Success Criteria

- All new files created with proper structure
- `OutputMode` enum defined and exported
- Colors module extracted
- Code compiles (may have stub implementations)
- No warnings


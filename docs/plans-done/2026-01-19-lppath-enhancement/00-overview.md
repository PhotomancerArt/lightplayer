# Plan: LpPath Enhancement

## Overview

Enhance `LpPath` with automatic normalization and comprehensive path manipulation methods to eliminate duplicate normalization logic and manual path string manipulation throughout the codebase.

This plan adds:
- Automatic path normalization on construction (like Rust's `PathBuf`)
- Support for both absolute and relative paths
- Path manipulation methods matching PathBuf API (`join`, `strip_prefix`, `parent`, `file_name`, `file_stem`, `extension`, `starts_with`, `ends_with`, `components`)
- Convenience method `join_relative()` for resolving relative paths
- Migration of manual path manipulation in `runtime.rs`, `server.rs`, and `project_manager.rs` to use new methods

## Phases

1. Enhance LpPath with normalization and core methods
2. Add path manipulation methods
3. Update runtime.rs to use join_relative()
4. Update server.rs to use strip_prefix()
5. Update project_manager.rs to use file_name()
6. Add tests and cleanup

## Success Criteria

- [ ] `LpPath` normalizes paths automatically on construction
- [ ] `LpPath` supports both absolute and relative paths
- [ ] All new path manipulation methods are implemented and match PathBuf API where applicable
- [ ] `runtime.rs` uses `join_relative()` for relative path resolution
- [ ] `server.rs` uses `strip_prefix()` for path manipulation
- [ ] `project_manager.rs` uses `file_name()` for extracting project names
- [ ] All existing tests pass
- [ ] New tests cover normalization and path manipulation
- [ ] Code compiles without warnings
- [ ] Code is formatted with `cargo +nightly fmt`

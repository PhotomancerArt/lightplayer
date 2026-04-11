# LpPath Enhancement Plan - Notes

## Questions

### Q1: Should LpPath normalize paths automatically on construction?

**Context**: Currently, `LpPath` is just a wrapper around `String` with no normalization. There are duplicate `normalize_path()` functions in `lp_fs_std.rs` and `lp_fs_mem.rs` that normalize paths (ensure leading `/`, collapse `//`, remove trailing `/`, etc.). 

**Options**:
- Option A: Normalize automatically via `From` implementations (like Rust's `PathBuf`)
- Option B: Require explicit normalization via `LpPath::normalize()` method
- Option C: Don't normalize, but provide normalization methods

**Answer**: Option A - normalize automatically on construction. This matches Rust's `PathBuf` behavior and ensures all `LpPath` instances are in a consistent normalized form. This eliminates the need for manual normalization throughout the codebase.

### Q2: Should LpPath support relative paths or only absolute?

**Context**: The current documentation says "Currently supports absolute paths only. Designed to support relative paths later when nodes become nestable." However, `runtime.rs` already manually handles relative paths for node resolution (e.g., `../output.output`). The filesystem abstraction (`LpFs`) always uses absolute paths from project root.

**Options**:
- Option A: Support both absolute and relative paths (like `PathBuf`)
- Option B: Only support absolute paths, but provide helper methods for relative path resolution

**Answer**: Option A - support both absolute and relative paths. This matches Rust's `PathBuf` and allows `LpPath` to be used for node specifiers and other cases where relative paths are needed. The filesystem can still require absolute paths when needed.

### Q3: Should we migrate filesystem normalization functions to use LpPath?

**Context**: There are duplicate `normalize_path()` functions in `lp_fs_std.rs` and `lp_fs_mem.rs`. The `LpFs` trait methods take `&str` parameters. We could either:
- Keep `LpFs` using `&str` and use `LpPath` internally for normalization
- Change `LpFs` to use `LpPath` (breaking change)
- Keep both - `LpFs` uses `&str`, but we use `LpPath` for internal path manipulation

**Answer**: Keep `LpFs` using `&str` for now. Don't refactor existing filesystem normalization functions yet. Focus on enhancing `LpPath` itself and using it in places that already use `LpPath` or new code.

### Q4: How should we handle path prefix stripping in server.rs?

**Context**: In `server.rs` lines 157-177, there's manual path manipulation to strip project prefixes and normalize paths. This could use `LpPath::strip_prefix()` if we add that method.

**Answer**: Add `strip_prefix()` method to `LpPath` that returns an `Option<LpPath>`. This matches Rust's `Path::strip_prefix()` API. Use it to replace the manual string manipulation in `server.rs`.

### Q5: How should we handle relative path resolution in runtime.rs?

**Context**: In `runtime.rs` lines 1145-1194, there's manual relative path resolution logic (splitting components, handling `.` and `..`, reconstructing). This is exactly what `LpPath::join_relative()` or `LpPath::join()` should handle.

**Answer**: Add `join_relative()` method that takes a relative path string and resolves it against an absolute `LpPath`. This handles `.` and `..` components correctly. Replace the manual logic in `runtime.rs` with a call to this method.

### Q6: Should we add a method to extract file name / last component?

**Context**: In `project_manager.rs` line 136-140, there's manual `rsplit('/')` logic to extract the last component (project name). Similar patterns exist elsewhere.

**Answer**: Add `file_name()` method (like `PathBuf::file_name()`) that returns `Option<&str>`. Also add `parent()` method. Use these to replace manual string manipulation.

### Q7: What about path joining for building paths?

**Context**: Throughout the codebase, paths are joined using `format!()` (e.g., `format!("{}/{}", base, name)`). This is error-prone and doesn't handle edge cases well.

**Answer**: Add `join()` method (like `PathBuf::join()`) that handles both absolute and relative paths correctly. If the joined path is absolute, it replaces the base. Otherwise, it appends. Use this throughout the codebase.

### Q8: Should we add a components iterator?

**Context**: Some code manually splits paths by `/` and filters empty components. A components iterator would be cleaner.

**Answer**: Add `components()` method that returns an iterator over path components (skipping empty ones and root `/`). This is useful for path manipulation and matches Rust's `Path::components()` API.

### Q9: How should we handle the transition period?

**Context**: We need to migrate existing code gradually. Some code uses `LpPath`, some uses `&str`. Filesystem code uses `&str`.

**Answer**: 
1. First, enhance `LpPath` with all the new methods
2. Update code that already uses `LpPath` to use the new methods (runtime.rs, server.rs, project_manager.rs)
3. Focus on the small refactors we've discussed - don't worry about migrating filesystem code since `LpFs` will be refactored separately anyway

### Q10: What about the watcher.rs normalize_path_sync function?

**Context**: `watcher.rs` has a `normalize_path_sync()` function that converts absolute filesystem paths to relative paths with leading `/`. This is different from `LpPath` normalization (which normalizes structure, not absolute→relative conversion).

**Answer**: Keep `normalize_path_sync()` as-is for now since it's doing filesystem-specific path conversion (absolute to relative). We can revisit this later if needed.

## Notes

- Current `LpPath` is very minimal - just a wrapper around `String`
- Lots of duplicate normalization logic across filesystem implementations
- Manual path manipulation is error-prone and inconsistent
- Need to support relative paths for node resolution
- Filesystem abstraction should continue using `&str` to avoid breaking changes
- Migration should be gradual, starting with code that already uses `LpPath`

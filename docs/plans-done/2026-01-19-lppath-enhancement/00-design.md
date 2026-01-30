# Design: LpPath Enhancement

## Overview

Enhance `LpPath` with automatic normalization and comprehensive path manipulation methods to eliminate duplicate normalization logic and manual path string manipulation throughout the codebase.

Key goals:
- Add automatic path normalization on construction (like Rust's `PathBuf`)
- Support both absolute and relative paths
- Add path manipulation methods (`join`, `join_relative`, `strip_prefix`, `parent`, `file_name`, `components`)
- Replace manual path manipulation in `runtime.rs`, `server.rs`, and `project_manager.rs`
- Keep `LpFs` using `&str` for now (will be refactored separately)

This improves code consistency, reduces errors from manual path manipulation, and provides a foundation for future path-related work.

## File Structure

```
lp-model/src/
└── path.rs                    # MODIFY: Enhance LpPath with normalization and path manipulation methods

lp-server/src/
├── server.rs                  # MODIFY: Use LpPath::strip_prefix() for path manipulation
└── project_manager.rs         # MODIFY: Use LpPath::file_name() instead of manual rsplit

lp-engine/src/project/
└── runtime.rs                 # MODIFY: Use LpPath::join_relative() for relative path resolution
```

## Type Tree

### lp-model/src/path.rs

- `pub struct LpPath` - **MODIFY**: Enhanced path type with automatic normalization
  ```rust
  pub struct LpPath(String);
  
  impl LpPath {
      // MODIFY: Normalize path on construction
      pub fn new(path: String) -> Self;
      
      // (no changes)
      pub fn as_str(&self) -> &str;
      
      // NEW: Check if path is absolute (starts with '/')
      pub fn is_absolute(&self) -> bool;
      
      // NEW: Check if path is relative (!starts with '/')
      pub fn is_relative(&self) -> bool;
      
      // NEW: Get parent directory path
      pub fn parent(&self) -> Option<LpPath>;
      
      // NEW: Get last component (file name)
      pub fn file_name(&self) -> Option<&str>;
      
      // NEW: Join paths (matches PathBuf::join - appends path, doesn't resolve ..)
      pub fn join<P: AsRef<str>>(&self, path: P) -> LpPath;
      
      // NEW: Join and resolve relative path (resolves . and .. components)
      // This is a convenience method beyond PathBuf API for resolving relative paths
      pub fn join_relative<P: AsRef<str>>(&self, path: P) -> Option<LpPath>;
      
      // NEW: Get file stem (file name without extension)
      pub fn file_stem(&self) -> Option<&str>;
      
      // NEW: Get file extension (without leading dot)
      pub fn extension(&self) -> Option<&str>;
      
      // NEW: Strip prefix from path
      pub fn strip_prefix<P: AsRef<str>>(&self, prefix: P) -> Option<LpPath>;
      
      // NEW: Check if path starts with base (base is a prefix)
      pub fn starts_with<P: AsRef<str>>(&self, base: P) -> bool;
      
      // NEW: Check if path ends with child (child is a suffix)
      pub fn ends_with<P: AsRef<str>>(&self, child: P) -> bool;
      
      // NEW: Iterator over path components
      pub fn components(&self) -> Components;
  }
  
  // NEW: Iterator type for path components
  pub struct Components<'a> {
      // Implements Iterator<Item = &'a str>
  }
  
  // MODIFY: Normalize on conversion
  impl From<String> for LpPath;
  impl From<&str> for LpPath;
  
  // Internal helper function
  fn normalize(path: &str) -> String;
  ```

### lp-server/src/server.rs

- `handle_fs_request()` - **MODIFY**: Use `LpPath::strip_prefix()` instead of manual string manipulation (lines 157-177)

### lp-server/src/project_manager.rs

- `load_project()` - **MODIFY**: Use `LpPath::file_name()` instead of manual `rsplit('/')` (line 136-140)

### lp-engine/src/project/runtime.rs

- `resolve_node()` - **MODIFY**: Use `LpPath::join_relative()` instead of manual relative path resolution (lines 1145-1194)

## Design Decisions

### 1. Automatic Normalization on Construction
**Decision**: Normalize paths automatically via `From` implementations, matching Rust's `PathBuf` behavior.

**Rationale**:
- Ensures all `LpPath` instances are in a consistent normalized form
- Eliminates need for manual normalization throughout codebase
- Matches familiar Rust standard library patterns
- Prevents bugs from inconsistent path formats

### 2. Support Both Absolute and Relative Paths
**Decision**: Support both absolute (starting with `/`) and relative paths.

**Rationale**:
- Matches Rust's `PathBuf` API
- Needed for node specifiers (e.g., `../output.output`)
- Filesystem can still require absolute paths when needed
- More flexible for future use cases

### 3. Keep LpFs Using &str
**Decision**: Don't refactor filesystem code to use `LpPath` yet.

**Rationale**:
- `LpFs` will be refactored separately
- Avoids breaking changes
- Focus on enhancing `LpPath` and updating existing `LpPath` usage first

### 4. Path Manipulation Methods
**Decision**: Add comprehensive path manipulation methods matching Rust's `Path`/`PathBuf` API, with `join_relative()` as a convenience method beyond the standard API.

**Rationale**:
- Familiar API for Rust developers (matches PathBuf method names)
- Eliminates error-prone manual string manipulation
- `join()` matches PathBuf behavior (appends without resolving `..`)
- `join_relative()` provides convenience for resolving relative paths (needed for runtime.rs)
- Handles edge cases correctly (`.`, `..`, multiple slashes, etc.)
- Makes code more readable and maintainable

## Implementation Notes

### Normalization Rules

The normalization function performs the following transformations:

1. Trim whitespace
2. Remove leading `./` or `.` (if present)
3. For absolute paths: ensure leading `/`
4. For relative paths: keep as-is (no leading `/`)
5. Collapse multiple consecutive slashes (`//` → `/`)
6. Remove trailing `/` unless it's the root path (`/`)
7. Handle empty paths: `""` → `"/"` (absolute root)

Examples:
- `"src/test"` → `"src/test"` (relative)
- `"/src/test"` → `"/src/test"` (absolute)
- `"//src//test//"` → `"/src/test"` (absolute, normalized)
- `"./src/test"` → `"src/test"` (relative)
- `""` → `"/"` (empty becomes root)

### Path Operations

#### `join(path)`
- Matches PathBuf::join behavior
- If `path` is absolute, replace base path
- If `path` is relative, append to base (does NOT resolve `..` components)
- Normalizes result

Examples:
- `LpPath::from("/src").join("test")` → `"/src/test"`
- `LpPath::from("/src").join("/test")` → `"/test"`
- `LpPath::from("/src/a").join("../b")` → `"/src/a/../b"` (doesn't resolve `..`)

#### `join_relative(path)`
- Convenience method beyond PathBuf API
- Similar to `join()` but resolves `.` and `..` components
- Returns `None` if result would be invalid (e.g., goes above root for absolute paths)
- Used for safe relative path resolution

Examples:
- `LpPath::from("/src/a").join_relative("../b")` → `Some("/src/b")`
- `LpPath::from("/src").join_relative("../../root")` → `None` (would go above root)

#### `join_relative(path)`
- Similar to `join()` but requires relative input
- Returns `None` if result would be invalid (e.g., goes above root for absolute paths)
- Used for safe relative path resolution

Examples:
- `LpPath::from("/src/a").join_relative("../b")` → `Some("/src/b")`
- `LpPath::from("/src").join_relative("../../root")` → `None` (would go above root)

#### `strip_prefix(prefix)`
- Remove prefix if path starts with it
- Returns `None` if prefix doesn't match
- Normalizes result

Examples:
- `LpPath::from("/projects/my-project/src").strip_prefix("/projects/my-project")` → `Some("/src")`
- `LpPath::from("/src").strip_prefix("/projects")` → `None`

#### `starts_with(base)`
- Check if path starts with the given base path (base is a prefix)
- Only considers whole path components to match
- Returns `bool`

Examples:
- `LpPath::from("/etc/passwd").starts_with("/etc")` → `true`
- `LpPath::from("/etc/passwd").starts_with("/etc/")` → `true`
- `LpPath::from("/etc/passwd").starts_with("/usr")` → `false`
- `LpPath::from("/etc/foo.rs").starts_with("/etc/foo")` → `false` (not a whole component)

#### `ends_with(child)`
- Check if path ends with the given child path (child is a suffix)
- Only considers whole path components to match
- Returns `bool`

Examples:
- `LpPath::from("/etc/resolv.conf").ends_with("resolv.conf")` → `true`
- `LpPath::from("/etc/resolv.conf").ends_with("etc/resolv.conf")` → `true`
- `LpPath::from("/etc/resolv.conf").ends_with("/etc/resolv.conf")` → `true`
- `LpPath::from("/etc/resolv.conf").ends_with("conf")` → `false` (not a whole component)

#### `components()`
- Iterate over non-empty path components
- Skips root `/` for absolute paths
- Returns iterator over `&str` slices

Examples:
- `LpPath::from("/src/test").components()` → `["src", "test"]`
- `LpPath::from("src/test").components()` → `["src", "test"]`

### Migration Strategy

1. Enhance `LpPath` with all new methods and normalization
2. Update existing `LpPath` usage:
   - `runtime.rs`: Replace manual relative path resolution with `join_relative()`
   - `server.rs`: Replace manual prefix stripping with `strip_prefix()`
   - `project_manager.rs`: Replace manual `rsplit('/')` with `file_name()`
3. Keep filesystem code unchanged (will be refactored separately)

## Error Handling

- `join_relative()`: Returns `None` if relative path resolution would result in an invalid path (e.g., going above root for absolute paths)
- `strip_prefix()`: Returns `None` if prefix doesn't match
- `starts_with()`: Returns `false` if base is not a prefix (always returns `bool`, never fails)
- `ends_with()`: Returns `false` if child is not a suffix (always returns `bool`, never fails)
- `parent()`: Returns `None` if path is root (`/`) or empty
- `file_name()`: Returns `None` if path is root (`/`) or empty
- `file_stem()`: Returns `None` if path is root (`/`) or empty, or if file_name has no extension
- `extension()`: Returns `None` if path is root (`/`) or empty, or if file_name has no extension
- Normalization: Never fails - always produces a valid normalized path

## Testing Strategy

### Unit Tests (`lp-model/src/path.rs`)

Test cases:

1. **Normalization tests**:
   - Test absolute path normalization (leading `/`, collapsing `//`, trailing `/`)
   - Test relative path normalization (no leading `/` added)
   - Test edge cases (empty string, `.`, `./`, root `/`)

2. **Path query tests**:
   - `is_absolute()` / `is_relative()` for various paths
   - `parent()` for various paths (including root)
   - `file_name()` for various paths (including root)
   - `file_stem()` for various paths (with and without extensions)
   - `extension()` for various paths (with and without extensions)

3. **Path manipulation tests**:
   - `join()` with absolute and relative paths
   - `join_relative()` with valid and invalid relative paths
   - `strip_prefix()` with matching and non-matching prefixes
   - `starts_with()` with various base paths
   - `ends_with()` with various child paths
   - `components()` iterator for various paths

4. **Integration tests**:
   - Test that `From` implementations normalize correctly
   - Test that all methods work together correctly

### Integration Tests

- Verify `runtime.rs` relative path resolution works correctly
- Verify `server.rs` prefix stripping works correctly
- Verify `project_manager.rs` file name extraction works correctly

## Success Criteria

- [ ] `LpPath` normalizes paths automatically on construction
- [ ] `LpPath` supports both absolute and relative paths
- [ ] All new path manipulation methods are implemented and tested
- [ ] `runtime.rs` uses `join_relative()` for relative path resolution
- [ ] `server.rs` uses `strip_prefix()` for path manipulation
- [ ] `project_manager.rs` uses `file_name()` for extracting project names
- [ ] All existing tests pass
- [ ] New tests cover normalization and path manipulation
- [ ] Code compiles without warnings
- [ ] Code is formatted with `cargo +nightly fmt`

## Notes

- Filesystem normalization functions (`lp_fs_std.rs`, `lp_fs_mem.rs`) remain unchanged - will be refactored separately
- `watcher.rs` `normalize_path_sync()` remains unchanged - does filesystem-specific conversion
- Normalization ensures consistent path format but doesn't validate path existence
- Path operations handle edge cases (`.`, `..`, multiple slashes) automatically
- All path operations normalize results to maintain consistency

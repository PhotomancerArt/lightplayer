# Design: Update LpFs to use LpPath/LpPathBuf split

## Overview

Create `LpPath` (slice type, like `Path`) and keep `LpPathBuf` (owned type, like `PathBuf`) to match Rust's `Path`/`PathBuf` pattern. Update `LpFs` trait and all implementations to use `P: AsRef<LpPath>` for path parameters. This provides proper type safety, eliminates duplicate normalization logic, and ensures all paths are normalized consistently.

Key goals:
- Create `LpPath` slice type (`#[repr(transparent)] pub struct LpPath(str)`)
- Update `LpFs` trait methods to accept `P: AsRef<LpPath>` instead of `&str`
- Change `list_dir()` to return `Vec<LpPathBuf>` instead of `Vec<String>`
- Implement `AsRef<LpPath>` for `&str`, `String`, `&LpPath`, and `LpPathBuf`
- Implement `Deref<Target = LpPath>` for `LpPathBuf`
- Move read-only methods to `LpPath`, mutation methods stay on `LpPathBuf`
- Remove all `normalize_path()` functions from implementations (redundant with `LpPathBuf` normalization)
- Update all call sites systematically to use `&LpPath`/`P: AsRef<LpPath>` in parameters and `LpPathBuf` for storage/returns

This improves type safety, eliminates normalization bugs, and makes the codebase consistent with Rust's path handling patterns.

## File Structure

```
lp-model/src/
└── path.rs                    # MODIFY: Create LpPath slice type, move methods, implement AsRef/Deref

lp-shared/src/fs/
├── lp_fs.rs                   # MODIFY: Update trait signatures to use P: AsRef<LpPath>
├── lp_fs_mem.rs               # MODIFY: Update implementation, remove normalize_path()
├── lp_fs_std.rs               # MODIFY: Update implementation, remove normalize_path()
└── lp_fs_view.rs              # MODIFY: Update implementation, remove normalize_path()

lp-server/src/
├── handlers.rs                 # MODIFY: Update to use LpPath/LpPathBuf
├── project_manager.rs          # MODIFY: Update to use LpPath/LpPathBuf
└── server.rs                   # MODIFY: Update to use LpPath/LpPathBuf

lp-engine/src/project/
├── loader.rs                   # MODIFY: Update to use LpPath/LpPathBuf
└── runtime.rs                  # MODIFY: Update to use LpPath/LpPathBuf

lp-client/src/
└── client.rs                  # MODIFY: Update to use LpPath/LpPathBuf

apps/lp-cli/src/
├── commands/dev/
│   ├── handler.rs             # MODIFY: Update to use LpPath/LpPathBuf
│   ├── pull_project.rs        # MODIFY: Update to use LpPath/LpPathBuf
│   ├── push_project.rs         # MODIFY: Update to use LpPath/LpPathBuf
│   ├── sync.rs                # MODIFY: Update to use LpPath/LpPathBuf
│   └── watcher.rs             # MODIFY: Update to use LpPath/LpPathBuf
├── commands/serve/
│   └── init.rs                 # MODIFY: Update to use LpPath/LpPathBuf
├── commands/create/
│   └── project.rs              # MODIFY: Update to use LpPath/LpPathBuf
├── server/
│   └── create_server.rs        # MODIFY: Update to use LpPath/LpPathBuf
└── client/
    └── client.rs              # MODIFY: Update to use LpPath/LpPathBuf

[Additional files with LpFs method calls - ~29 files total]
```

## Type Tree

### lp-model/src/path.rs

- `pub struct LpPath` - **NEW**: Slice type (like `Path`)
  ```rust
  #[repr(transparent)]
  pub struct LpPath(str);
  
  impl LpPath {
      /// Create a new LpPath from a string slice (cost-free, no normalization)
      pub fn new(s: &str) -> &LpPath {
          // Unsafe cast (like Path::new)
      }
      
      /// Get the path as a string slice
      pub fn as_str(&self) -> &str;
      
      // Read-only methods (moved from LpPathBuf):
      pub fn is_absolute(&self) -> bool;
      pub fn is_relative(&self) -> bool;
      pub fn parent(&self) -> Option<&LpPath>;  // Returns borrowed view
      pub fn file_name(&self) -> Option<&str>;
      pub fn file_stem(&self) -> Option<&str>;
      pub fn extension(&self) -> Option<&str>;
      pub fn strip_prefix<P: AsRef<str>>(&self, prefix: P) -> Option<&LpPath>;
      pub fn starts_with<P: AsRef<str>>(&self, base: P) -> bool;
      pub fn ends_with<P: AsRef<str>>(&self, child: P) -> bool;
      pub fn components(&self) -> Components<'_>;
  }
  
  // NEW: Implement AsRef<LpPath> for various types
  impl AsRef<LpPath> for &str {
      fn as_ref(&self) -> &LpPath {
          LpPath::new(self)  // Unsafe cast
      }
  }
  
  impl AsRef<LpPath> for String {
      fn as_ref(&self) -> &LpPath {
          LpPath::new(self.as_str())
      }
  }
  
  impl AsRef<LpPath> for &LpPath {
      fn as_ref(&self) -> &LpPath {
          self
      }
  }
  
  impl AsRef<LpPath> for LpPathBuf {
      fn as_ref(&self) -> &LpPath {
          self.deref()
      }
  }
  ```

- `pub struct LpPathBuf` - **MODIFY**: Owned type (like `PathBuf`)
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub struct LpPathBuf(String);
  
  impl LpPathBuf {
      /// Create a new LpPathBuf, normalizing the path
      pub fn new(path: String) -> Self;
      
      /// Get the path as a string slice
      pub fn as_str(&self) -> &str;
      
      /// Get as &LpPath
      pub fn as_path(&self) -> &LpPath;
      
      // Mutation methods (stay on LpPathBuf):
      pub fn join<P: AsRef<str>>(&self, path: P) -> LpPathBuf;
      pub fn join_relative<P: AsRef<str>>(&self, path: P) -> Option<LpPathBuf>;
      // ... other mutation methods
  }
  
  // NEW: Implement Deref so LpPathBuf can use LpPath methods
  impl Deref for LpPathBuf {
      type Target = LpPath;
      fn deref(&self) -> &LpPath {
          LpPath::new(&self.0)
      }
  }
  
  // MODIFY: Normalize on conversion (existing behavior)
  impl From<String> for LpPathBuf;
  impl From<&str> for LpPathBuf;
  ```

### lp-shared/src/fs/lp_fs.rs

- `pub trait LpFs` - **MODIFY**: Update all method signatures
  ```rust
  pub trait LpFs {
      // MODIFY: Change path parameter from &str to P: AsRef<LpPath>
      fn read_file<P: AsRef<LpPath>>(&self, path: P) -> Result<Vec<u8>, FsError>;
      
      fn write_file<P: AsRef<LpPath>>(&self, path: P, data: &[u8]) -> Result<(), FsError>;
      
      fn file_exists<P: AsRef<LpPath>>(&self, path: P) -> Result<bool, FsError>;
      
      fn is_dir<P: AsRef<LpPath>>(&self, path: P) -> Result<bool, FsError>;
      
      // MODIFY: Change path parameter and return type
      fn list_dir<P: AsRef<LpPath>>(
          &self,
          path: P,
          recursive: bool,
      ) -> Result<Vec<LpPathBuf>, FsError>;
      
      fn delete_file<P: AsRef<LpPath>>(&self, path: P) -> Result<(), FsError>;
      
      fn delete_dir<P: AsRef<LpPath>>(&self, path: P) -> Result<(), FsError>;
      
      // MODIFY: Change subdir parameter
      fn chroot<P: AsRef<LpPath>>(
          &self,
          subdir: P
      ) -> Result<Rc<RefCell<dyn LpFs>>, FsError>;
      
      // (no changes - these don't take paths)
      fn current_version(&self) -> FsVersion;
      fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange>;
      fn clear_changes_before(&mut self, before_version: FsVersion);
      fn record_changes(&mut self, changes: Vec<FsChange>);
  }
  ```

### lp-shared/src/fs/lp_fs_mem.rs

- `impl LpFs for LpFsMemory` - **MODIFY**: Update implementations, remove `normalize_path()`
  ```rust
  impl LpFs for LpFsMemory {
      fn read_file<P: AsRef<LpPath>>(&self, path: P) -> Result<Vec<u8>, FsError> {
          let lp_path = path.as_ref();
          // Convert to LpPathBuf for normalization, then use as_str()
          let normalized = LpPathBuf::from(lp_path.as_str());
          let normalized_str = normalized.as_str();
          // Use normalized_str for lookup
      }
      
      // Similar changes for all other methods
      // REMOVE: normalize_path() function (no longer needed)
  }
  ```

### lp-shared/src/fs/lp_fs_std.rs

- `impl LpFs for LpFsStd` - **MODIFY**: Update implementations, remove `normalize_path()`
  ```rust
  impl LpFs for LpFsStd {
      fn read_file<P: AsRef<LpPath>>(&self, path: P) -> Result<Vec<u8>, FsError> {
          let lp_path = path.as_ref();
          // Convert to LpPathBuf for normalization
          let normalized = LpPathBuf::from(lp_path.as_str());
          let normalized_str = normalized.as_str();
          // Use normalized_str for path resolution
      }
      
      // Similar changes for all other methods
      // REMOVE: normalize_path() function (no longer needed)
  }
  ```

### lp-shared/src/fs/lp_fs_view.rs

- `impl LpFs for LpFsView` - **MODIFY**: Update implementations, remove `normalize_path()`
  ```rust
  impl LpFs for LpFsView {
      fn read_file<P: AsRef<LpPath>>(&self, path: P) -> Result<Vec<u8>, FsError> {
          let lp_path = path.as_ref();
          // Convert to LpPathBuf for normalization
          let normalized = LpPathBuf::from(lp_path.as_str());
          let normalized_str = normalized.as_str();
          // Use normalized_str for path translation
      }
      
      // MODIFY: list_dir() return type to Vec<LpPathBuf>
      fn list_dir<P: AsRef<LpPath>>(
          &self,
          path: P,
          recursive: bool,
      ) -> Result<Vec<LpPathBuf>, FsError> {
          // Convert parent paths to LpPathBuf before returning
      }
      
      // Similar changes for all other methods
      // REMOVE: normalize_path() function (no longer needed)
  }
  ```

## Implementation Notes

### Creating LpPath Slice Type

The `LpPath` slice type uses `#[repr(transparent)]` to ensure it has the same memory layout as `str`:

```rust
#[repr(transparent)]
pub struct LpPath(str);

impl LpPath {
    pub fn new(s: &str) -> &LpPath {
        unsafe { &*(s as *const str as *const LpPath) }
    }
}
```

This allows safe casting between `&str` and `&LpPath` since they have identical memory layouts.

### Normalization Strategy

- `LpPath::new()` does NOT normalize (matches Rust's `Path::new()`)
- `LpPathBuf::from()` DOES normalize (custom behavior, ensures consistency)
- `LpFs` implementations convert `&LpPath` → `LpPathBuf` for normalization, then use `as_str()`

### Method Organization

- Read-only methods (`is_absolute`, `file_name`, `parent`, etc.) → `LpPath`
- Mutation methods (`join`, `join_relative`, etc.) → `LpPathBuf`
- `LpPathBuf` implements `Deref<Target = LpPath>` so it can use `LpPath` methods

### Return Types

- Methods that return views into the original path → `&LpPath` (e.g., `parent()`)
- Methods that build/combine paths → `LpPathBuf` (e.g., `join()`)
- `list_dir()` → `Vec<LpPathBuf>` (callers own the paths)

### Serialization

- Only `LpPathBuf` implements `Serialize/Deserialize`
- `LpPath` cannot be serialized (unsized type)
- When serializing, convert `&LpPath` to `LpPathBuf` first

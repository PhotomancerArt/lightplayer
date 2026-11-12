# Phase 4: Update LpFs trait signatures

## Description

Update the `LpFs` trait to use `P: AsRef<LpPath>` for all path parameters and change `list_dir()` return type to `Vec<LpPathBuf>`.

## Implementation

1. Update `lp-shared/src/fs/lp_fs.rs`:
   - Add `use lp_model::path::{LpPath, LpPathBuf};`
   - Update all method signatures:
     - `read_file<P: AsRef<LpPath>>(&self, path: P) -> Result<Vec<u8>, FsError>`
     - `write_file<P: AsRef<LpPath>>(&self, path: P, data: &[u8]) -> Result<(), FsError>`
     - `file_exists<P: AsRef<LpPath>>(&self, path: P) -> Result<bool, FsError>`
     - `is_dir<P: AsRef<LpPath>>(&self, path: P) -> Result<bool, FsError>`
     - `list_dir<P: AsRef<LpPath>>(&self, path: P, recursive: bool) -> Result<Vec<LpPathBuf>, FsError>`
     - `delete_file<P: AsRef<LpPath>>(&self, path: P) -> Result<(), FsError>`
     - `delete_dir<P: AsRef<LpPath>>(&self, path: P) -> Result<(), FsError>`
     - `chroot<P: AsRef<LpPath>>(&self, subdir: P) -> Result<Rc<RefCell<dyn LpFs>>, FsError>`
   - Update documentation to reflect new parameter types

## Success Criteria

- `LpFs` trait updated with new signatures
- Code compiles (implementations will be updated in next phase)
- Documentation updated

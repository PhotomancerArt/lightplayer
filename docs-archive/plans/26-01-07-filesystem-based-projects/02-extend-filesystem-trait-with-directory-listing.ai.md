# Phase 2: Extend Filesystem Trait with Directory Listing and Path Semantics

## Goal

Add directory listing capability to the Filesystem trait and document path semantics.

## Tasks

1. Add `list_dir()` method to `Filesystem` trait in `fs/trait.rs`:
   ```rust
   /// List directory contents (files and subdirectories)
   /// Path is relative to project root (e.g., "/src" or "/src/nested")
   /// Returns paths relative to project root
   fn list_dir(&self, path: &str) -> Result<Vec<String>, Error>;
   ```

2. Update trait documentation to clarify path semantics:
   - All paths are relative to project root
   - `/project.json` is always the project configuration file
   - Leading slash indicates path from project root
   - Filesystem instances have a root path for security

3. Update existing method documentation to clarify path semantics

## Success Criteria

- `list_dir()` method added to Filesystem trait
- Path semantics documented
- Code compiles (implementation will come in later phases)
- No warnings


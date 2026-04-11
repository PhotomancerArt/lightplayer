# Phase 13: Implement host filesystem abstraction

## Goal

Implement filesystem access on host using std::fs.

## Tasks

1. Create `src/filesystem.rs`:
   - Implement `lp-core::traits::Filesystem` trait for host
   - Use `std::fs` for file operations
   - Handle file paths (use current directory or configurable path)
2. Initialize filesystem in `main.rs`:
   - Create filesystem instance
   - Ensure project directory exists
3. Test basic file read/write operations

## Success Criteria

- Filesystem trait implementation works
- Can read/write files on host
- All code compiles without warnings


# Phase 2: Add path manipulation methods

## Description

Add path manipulation methods (`join`, `join_relative`, `strip_prefix`, `starts_with`, `ends_with`, `components`) matching PathBuf API where applicable.

## Implementation

### 1. Add join() method

- Matches PathBuf::join behavior
- If `path` is absolute, replace base path
- If `path` is relative, append to base (does NOT resolve `..` components)
- Normalizes result

### 2. Add join_relative() method

- Convenience method beyond PathBuf API
- Similar to `join()` but resolves `.` and `..` components
- Returns `None` if result would be invalid (e.g., goes above root for absolute paths)
- Used for safe relative path resolution

### 3. Add strip_prefix() method

- Remove prefix if path starts with it
- Returns `None` if prefix doesn't match
- Normalizes result
- Matches PathBuf API (but returns `Option<LpPath>` instead of `Result` for simplicity)

### 4. Add starts_with() method

- Check if path starts with the given base path (base is a prefix)
- Only considers whole path components to match
- Returns `bool`
- Matches PathBuf API

### 5. Add ends_with() method

- Check if path ends with the given child path (child is a suffix)
- Only considers whole path components to match
- Returns `bool`
- Matches PathBuf API

### 6. Add components() iterator

- Iterate over non-empty path components
- Skips root `/` for absolute paths
- Returns iterator over `&str` slices
- Matches PathBuf API

## Success Criteria

- [ ] `join()` matches PathBuf behavior (appends without resolving `..`)
- [ ] `join_relative()` resolves `.` and `..` components correctly
- [ ] `join_relative()` returns `None` for invalid paths (going above root)
- [ ] `strip_prefix()` removes matching prefixes correctly
- [ ] `strip_prefix()` returns `None` for non-matching prefixes
- [ ] `starts_with()` checks whole path components correctly
- [ ] `ends_with()` checks whole path components correctly
- [ ] `components()` iterator works correctly for absolute and relative paths
- [ ] Code compiles without errors
- [ ] Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language
- Avoid emoticons
- Use measured, factual descriptions

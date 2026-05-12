# Phase 1: Enhance LpPath with normalization and core methods

## Description

Add automatic path normalization on construction and implement core path query methods (`is_absolute`, `is_relative`, `parent`, `file_name`, `file_stem`, `extension`).

## Implementation

### 1. Add normalization function

Add internal `normalize()` function that:
- Trims whitespace
- Removes leading `./` or `.` (if present)
- For absolute paths: ensures leading `/`
- For relative paths: keeps as-is (no leading `/`)
- Collapses multiple consecutive slashes (`//` → `/`)
- Removes trailing `/` unless it's the root path (`/`)
- Handles empty paths: `""` → `"/"` (absolute root)

### 2. Update From implementations

Modify `From<String>` and `From<&str>` implementations to normalize paths on construction.

### 3. Update new() method

Modify `LpPath::new()` to normalize paths.

### 4. Add core query methods

- `is_absolute() -> bool` - Check if path is absolute (starts with `/`)
- `is_relative() -> bool` - Check if path is relative (!starts with `/`)
- `parent() -> Option<LpPath>` - Get parent directory path
- `file_name() -> Option<&str>` - Get last component (file name)
- `file_stem() -> Option<&str>` - Get file name without extension
- `extension() -> Option<&str>` - Get file extension (without leading dot)

## Success Criteria

- [ ] Paths are normalized automatically on construction via `From` implementations
- [ ] `LpPath::new()` normalizes paths
- [ ] `is_absolute()` and `is_relative()` work correctly
- [ ] `parent()` returns correct parent path or `None` for root
- [ ] `file_name()` returns last component or `None` for root
- [ ] `file_stem()` extracts file name without extension correctly
- [ ] `extension()` extracts file extension correctly
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

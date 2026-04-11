# Phase 6: Implement error types and Display trait

## Goal

Create a comprehensive error type system for lp-core.

## Tasks

1. Create `src/error.rs` with:
   - `Error` enum covering:
     - Filesystem errors
     - Serialization/deserialization errors
     - Protocol errors
     - Project validation errors
     - Node-specific errors
   - Implement `Display` trait for `Error`
   - Implement `std::error::Error` trait (if possible in no_std, otherwise skip)
   - Helper functions for creating errors
2. Use `Result<T, Error>` pattern throughout codebase
3. Update existing code to use new error types

## Success Criteria

- Error types cover all error cases
- Errors can be displayed/formatted
- Error types are used consistently
- All code compiles without warnings


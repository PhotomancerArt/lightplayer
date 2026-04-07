# Phase 1: Create lpvm/src/abi.rs

## Scope

Create the ABI marshaling module in `lpvm` crate with `CallError`, `GlslReturn`, and placeholder `flatten_q32`/`unflatten_q32` functions. This establishes the error type and return wrapper that other modules will depend on.

## Code Organization

- Place module doc comment at top
- Error type first (entry point for failures)
- Return wrapper next
- Placeholder flatten/unflatten functions (will be implemented in phase 2)

## Implementation

```rust
//! ABI marshaling for Q32 function calls
//!
//! Converts between structured LpsValueQ32 and flat i32 words
//! for JIT/emulator calling conventions.

use alloc::string::String;
use alloc::vec::Vec;
use lps_shared::{LpsType, LpsValueQ32};

/// Error during ABI marshaling or call
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    MissingMetadata(String),
    Arity { expected: usize, got: usize },
    TypeMismatch(String),
    Unsupported(String),
}

impl core::fmt::Display for CallError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CallError::MissingMetadata(n) => write!(f, "no metadata for `{n}`"),
            CallError::Arity { expected, got } => {
                write!(f, "wrong argument count: expected {expected}, got {got}")
            }
            CallError::TypeMismatch(s) | CallError::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CallError {}

/// Result of a shader call: optional returned value plus out/inout values
#[derive(Clone, Debug, PartialEq)]
pub struct GlslReturn<V> {
    pub value: Option<V>,
    pub outs: Vec<V>,
}

/// Flatten LpsValueQ32 to raw i32 words for JIT/emulator ABI
///
/// # Phase 2
/// TODO: Implement using logic from old flatten_q32_arg
pub fn flatten_q32(_ty: &LpsType, _v: &LpsValueQ32) -> Result<Vec<i32>, CallError> {
    todo!("Implemented in phase 2")
}

/// Unflatten raw i32 words from JIT/emulator to LpsValueQ32
///
/// # Phase 2
/// TODO: Implement using logic from old decode_q32_return
pub fn unflatten_q32(_ty: &LpsType, _words: &[i32]) -> Result<LpsValueQ32, CallError> {
    todo!("Implemented in phase 2")
}
```

## Validate

```bash
cargo check -p lpvm
```

Should compile with warnings about unused functions (expected - will be used in later phases).

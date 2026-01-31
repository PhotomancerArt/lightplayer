# Phase 1: Create fw-core crate structure and SerialIo trait

## Scope of phase

Create the `fw-core` crate with basic structure and the `SerialIo` trait definition. This establishes the foundation for serial communication abstractions.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create crate structure

Create `lp-app/crates/fw-core/` directory with:

- `Cargo.toml` - Crate configuration
- `src/lib.rs` - Library entry point
- `src/serial/mod.rs` - Serial module
- `src/serial/io.rs` - SerialIo trait
- `src/transport/mod.rs` - Transport module (empty for now)

### 2. Cargo.toml

```toml
[package]
name = "fw-core"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = []
std = []

[dependencies]
lp-model = { path = "../lp-model", default-features = false }
lp-shared = { path = "../lp-shared", default-features = false }
lp-server = { path = "../lp-server", default-features = false }
serde_json = { workspace = true, default-features = false, features = ["alloc"] }

[dev-dependencies]
lp-shared = { path = "../lp-shared", default-features = false, features = ["std"] }
lp-server = { path = "../lp-server", default-features = false, features = ["std"] }
```

### 3. lib.rs

```rust
#![no_std]

pub mod serial;
pub mod transport;
```

### 4. serial/mod.rs

```rust
pub mod io;

pub use io::{SerialError, SerialIo};
```

### 5. serial/io.rs

Define the `SerialIo` trait:

```rust
//! Serial I/O trait for firmware communication
//!
//! Provides a simple, synchronous interface for reading and writing raw bytes.
//! The transport layer handles message framing, buffering, and JSON parsing.

/// Error type for serial I/O operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    /// Write operation failed
    WriteFailed(alloc::string::String),
    /// Read operation failed
    ReadFailed(alloc::string::String),
    /// Other serial error
    Other(alloc::string::String),
}

impl core::fmt::Display for SerialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerialError::WriteFailed(msg) => write!(f, "Write failed: {}", msg),
            SerialError::ReadFailed(msg) => write!(f, "Read failed: {}", msg),
            SerialError::Other(msg) => write!(f, "Serial error: {}", msg),
        }
    }
}

/// Trait for serial I/O operations
///
/// Provides a simple, synchronous interface for reading and writing raw bytes.
/// Implementations can use blocking or async I/O internally, but the interface
/// is synchronous to keep the transport layer simple.
pub trait SerialIo {
    /// Write bytes to the serial port (blocking)
    ///
    /// This is a blocking operation that writes all bytes before returning.
    /// For async implementations, this can be a wrapper that blocks on the async write.
    ///
    /// # Arguments
    /// * `data` - Bytes to write
    ///
    /// # Returns
    /// * `Ok(())` if all bytes were written successfully
    /// * `Err(SerialError)` if writing failed
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError>;

    /// Read available bytes from the serial port (non-blocking)
    ///
    /// Reads up to `buf.len()` bytes that are currently available.
    /// Returns immediately with whatever data is available (may be 0 bytes).
    /// Does not block waiting for data.
    ///
    /// # Arguments
    /// * `buf` - Buffer to read into
    ///
    /// # Returns
    /// * `Ok(n)` - Number of bytes read (0 if no data available)
    /// * `Err(SerialError)` if reading failed
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError>;

    /// Check if data is available to read (optional optimization)
    ///
    /// Returns `true` if `read_available()` would return at least 1 byte.
    /// This is an optimization hint - implementations can always return `true`
    /// and let `read_available()` return 0 if no data is available.
    ///
    /// # Returns
    /// * `true` if data is available
    /// * `false` if no data is available
    fn has_data(&self) -> bool {
        // Default implementation always returns true
        // Implementations can override for optimization
        true
    }
}
```

### 6. transport/mod.rs

```rust
// Transport module - will be implemented in next phase
```

## Tests

Add basic unit tests for `SerialError`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_error_display() {
        let err = SerialError::WriteFailed("test".into());
        assert!(format!("{}", err).contains("Write failed"));
    }
}
```

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-core
cargo test --package fw-core
```

Ensure:

- Crate compiles with `no_std`
- All tests pass
- No warnings

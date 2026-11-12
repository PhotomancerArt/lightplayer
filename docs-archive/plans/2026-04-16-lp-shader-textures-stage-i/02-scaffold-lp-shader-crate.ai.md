# Phase 2 — Scaffold lp-shader crate

## Scope

Create the `lp-shader/lp-shader/` directory with `Cargo.toml` and minimal
`src/lib.rs` + `src/error.rs`. Add to workspace members and default-members.
Ensure the crate compiles (empty, with deps wired).

## Code organization reminders

- One concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom.
- Any temporary code should have a TODO comment.

## Implementation details

### `lp-shader/lp-shader/Cargo.toml`

```toml
[package]
name = "lp-shader"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "High-level shader compilation and texture API"

[lints]
workspace = true

[features]
default = []
std = []

[dependencies]
lps-shared = { path = "../lps-shared" }
lpir = { path = "../lpir" }
lpvm = { path = "../lpvm" }
lps-frontend = { path = "../lps-frontend" }
```

### `lp-shader/lp-shader/src/error.rs`

```rust
use alloc::string::String;
use core::fmt;

/// Errors from the lp-shader compilation and rendering pipeline.
#[derive(Debug)]
pub enum LpsError {
    /// GLSL parse failure (naga frontend).
    Parse(String),
    /// LPIR lowering failure.
    Lower(String),
    /// Backend compilation failure.
    Compile(String),
    /// Render-time failure (trap, type mismatch, etc.).
    Render(String),
}

impl fmt::Display for LpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LpsError::Parse(msg) => write!(f, "parse: {msg}"),
            LpsError::Lower(msg) => write!(f, "lower: {msg}"),
            LpsError::Compile(msg) => write!(f, "compile: {msg}"),
            LpsError::Render(msg) => write!(f, "render: {msg}"),
        }
    }
}
```

### `lp-shader/lp-shader/src/lib.rs`

```rust
#![no_std]

extern crate alloc;

mod error;

pub use error::LpsError;

// Re-exports from lps-shared
pub use lps_shared::{
    LpsModuleSig, LpsValueF32, TextureBuffer, TextureStorageFormat,
};
```

### Workspace `Cargo.toml`

Add `"lp-shader/lp-shader"` to both `members` and `default-members` arrays.
Insert alphabetically near the other `lp-shader/` entries.

## Validate

```bash
cargo check -p lp-shader
cargo check  # full default workspace
```

# Phase 1: Crate Scaffold + Workspace Integration

## Goal
Create the `lpfx/lpfx` crate directory structure and wire it into the
workspace so `cargo check -p lpfx` works.

## Steps

### 1.1 Create directory structure
```
lpfx/
└── lpfx/
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

### 1.2 Write `lpfx/lpfx/Cargo.toml`
```toml
[package]
name = "lpfx"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true, features = ["derive"] }
toml = { workspace = true }
```

Follow workspace convention — `serde` is already
`default-features = false, features = ["alloc"]` at the workspace level.

### 1.3 Write initial `lpfx/lpfx/src/lib.rs`
```rust
#![no_std]
extern crate alloc;
```

### 1.4 Update root `Cargo.toml`
- Add `"lpfx/lpfx"` to `[workspace] members` and `default-members`.
- Add `toml = { version = "0.9", default-features = false }` to
  `[workspace.dependencies]`.

### 1.5 Verify
```
cargo check -p lpfx
```
Must compile clean with `no_std`.

## Validation
- `cargo check -p lpfx` succeeds.
- `cargo build -p lpfx` succeeds.

# Phase 7: Make lpvm-cranelift Optional

## Scope

Feature-gate `lpvm-cranelift` as an optional dependency in `lp-engine`, making it a compile-time choice while keeping `LpGraphics` always available.

## Files

```
lp-core/lp-engine/Cargo.toml        # UPDATE: make lpvm-cranelift optional
```

## Cargo.toml Changes

```toml
[dependencies]
# ... existing deps ...

# Graphics backends (optional, but at least one needed for shader support)
lpvm-cranelift = { path = "../../lp-shader/lpvm-cranelift", optional = true }

[features]
default = []  # No graphics backends by default - firmware chooses

# Graphics backends
cranelift = ["dep:lpvm-cranelift", "lpvm-cranelift?/std"]
wasm = []  # Reserved for future lpvm-wasm backend

# Convenience feature for tests that need Cranelift
test-cranelift = ["cranelift"]
```

## gfx/cranelift.rs conditional compilation

```rust
// In lp-engine/src/gfx/cranelift.rs
#![cfg(feature = "cranelift")]

use lpvm_cranelift::...;
// ... rest of implementation
```

## gfx/mod.rs re-export

```rust
// In lp-engine/src/gfx/mod.rs
pub mod lp_gfx;
pub mod lp_shader;

// Conditionally re-export backend modules
#[cfg(feature = "cranelift")]
pub mod cranelift;
```

## Update firmware features

```toml
# In fw-esp32/Cargo.toml
[features]
default = ["cranelift"]
cranelift = ["lp-engine/cranelift", "lp-server/cranelift"]

# In fw-emu/Cargo.toml  
[features]
default = ["cranelift"]
cranelift = ["lp-engine/cranelift", "lp-server/cranelift"]
```

## Update lp-server features (if needed)

If `lp-server` re-exports anything from `lp-engine`:

```toml
# In lp-server/Cargo.toml
[features]
cranelift = ["lp-engine/cranelift"]
```

## Validation

```bash
# Check with cranelift feature (default)cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Check without cranelift (should still compile, shaders won't work)
cargo check -p lp-engine --no-default-features --lib

# Host build for tests
cargo check -p lp-engine --features test-cranelift --lib
```

## Notes

- `lp-engine` compiles without any graphics backend, but shader nodes will fail at runtime
- Firmware crates **must** enable at least one backend feature or runtime shader calls will panic
- This sets up for future WASM backend — just add `wasm = ["dep:lpvm-wasm"]` etc

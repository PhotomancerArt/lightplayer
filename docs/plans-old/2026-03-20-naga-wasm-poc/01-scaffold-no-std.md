# Phase 1: Scaffold crate + validate no_std compilation

## Scope

Create the spike crate, add it to the workspace, configure Naga as a path dep
with `glsl-in`, mark the lib as `#![no_std]`, and verify it compiles. This
phase answers the key question: does `naga` with `glsl-in` work under no_std?

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation details

### 1. Create `spikes/naga-wasm-poc/Cargo.toml`

```toml
[package]
name = "naga-wasm-poc"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
naga = { path = "../../oss/wgpu/naga", default-features = false, features = ["glsl-in"] }
wasm-encoder = "0.245"

[dev-dependencies]
wasmtime = "42"
```

### 2. Create `spikes/naga-wasm-poc/src/lib.rs`

```rust
#![no_std]
extern crate alloc;
```

Just the no_std declaration. If this compiles, Naga's glsl-in is no_std
compatible. If it fails, we know immediately what needs forking.

### 3. Add to workspace

Add `"spikes/naga-wasm-poc"` to the `[workspace] members` list in the root
`Cargo.toml`.

### 4. Verify compilation

```bash
cargo check -p naga-wasm-poc
```

If this fails due to std in naga or pp-rs, document the error and stop — that's
a finding. If it compiles, the no_std question is answered.

## Validate

```bash
cargo check -p naga-wasm-poc
```

Must compile with no errors. Warnings are acceptable at this phase.

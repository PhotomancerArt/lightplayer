# Phase 2: `lpfx-cpu` crate scaffold + workspace

## Scope

Create `lpfx/lpfx-cpu` with `Cargo.toml` (feature-gated backend deps),
wire into workspace, and stub `CpuFxEngine` / `CpuFxInstance`.

## Code organization reminders

- One concept per file.
- Place traits/entry points first, helpers at the bottom.
- Keep related functionality grouped together.

## Implementation

### 2.1 `lpfx/lpfx-cpu/Cargo.toml`

```toml
[package]
name = "lpfx-cpu"
description = "CPU rendering backend for lpfx effect modules (no_std + alloc)"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["cranelift"]
cranelift = ["dep:lpvm-cranelift"]
# native = ["dep:lpvm-native"]    # future: rv32
# wasm = ["dep:lpvm-wasm"]        # future: browser

[dependencies]
lpfx = { path = "../lpfx" }
lpvm = { path = "../../lp-shader/lpvm", default-features = false }
lps-frontend = { path = "../../lp-shader/lps-frontend", default-features = false }
lps-shared = { path = "../../lp-shader/lps-shared", default-features = false }
log = { workspace = true, default-features = false }

# Backend deps (optional, pulled by features)
lpvm-cranelift = { path = "../../lp-shader/lpvm-cranelift", optional = true, default-features = false, features = ["glsl"] }
# lpvm-native = { path = "../../lp-shader/lpvm-native", optional = true, default-features = false }
# lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", optional = true, default-features = false }

[lints]
workspace = true
```

Note: `lps-shared` is needed for `LpsModuleSig`, `LpsValueF32`, layout
utilities used in uniform encoding. It is `no_std`.

### 2.2 `lpfx/lpfx-cpu/src/lib.rs`

Initial stub:

```rust
#![no_std]
extern crate alloc;

mod compile;
#[cfg(feature = "cranelift")]
mod render_cranelift;

use alloc::collections::BTreeMap;

use lpfx::texture::{CpuTexture, TextureFormat, TextureId};
use lpfx::engine::{FxEngine, FxInstance};
use lpfx::input::FxValue;
use lpfx::module::FxModule;

pub struct CpuFxEngine {
    textures: BTreeMap<TextureId, CpuTexture>,
    next_id: u32,
}

impl CpuFxEngine {
    pub fn new() -> Self {
        Self { textures: BTreeMap::new(), next_id: 0 }
    }

    /// Read-only access to a texture's pixel data.
    pub fn texture(&self, id: TextureId) -> Option<&CpuTexture> {
        self.textures.get(&id)
    }

    /// Mutable access to a texture (for the render loop to write pixels).
    pub fn texture_mut(&mut self, id: TextureId) -> Option<&mut CpuTexture> {
        self.textures.get_mut(&id)
    }
}
```

Trait impls and `CpuFxInstance` will be filled in phases 3-4.

### 2.3 Update root `Cargo.toml`

Add `"lpfx/lpfx-cpu"` to `members` and `default-members`.

### 2.4 Stub `compile.rs`

Empty module for now:

```rust
//! GLSL -> LPIR -> compiled module, with input-to-uniform validation.
```

## Validate

```bash
cargo check -p lpfx-cpu
cargo check
```

The crate compiles (with stubs). No tests yet -- those come in phase 5.

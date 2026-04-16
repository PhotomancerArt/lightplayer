## Phase 1: Create lpvm-wasm Crate Skeleton

### Scope

Create the crate directory structure, Cargo.toml with dependencies, and empty
module files. No implementation yet — just the scaffold.

### Implementation Details

**Directory structure:**

```
lpvm/lpvm-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── emit.rs
    ├── emit/
    │   ├── mod.rs
    │   ├── control.rs
    │   ├── func.rs
    │   ├── imports.rs
    │   ├── memory.rs
    │   ├── ops.rs
    │   └── q32.rs
    ├── module.rs
    ├── options.rs
    ├── error.rs
    └── runtime/
        ├── mod.rs
        ├── engine.rs
        └── instance.rs
```

**Cargo.toml:**

```toml
[package]
name = "lpvm-wasm"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "LPVM WASM backend - LPIR to WebAssembly"

[lints]
workspace = true

[features]
default = []
# Runtime support via wasmtime (host only)
runtime = ["dep:wasmtime", "std"]
# Enable std for host builds (runtime feature implies this)
std = []

[dependencies]
# Core dependencies (always, no_std + alloc)
lpir = { path = "../../lp-shader/lpir" }
lps-shared = { path = "../../lp-shader/lps-shared" }
lpvm = { path = "../../lp-shader/lpvm" }
wasm-encoder = "0.245"

# Runtime dependencies (runtime feature only)
wasmtime = { version = "42", optional = true }
```

**lib.rs scaffold:**

```rust
//! LPVM WASM backend - LPIR to WebAssembly emission and runtime.

#![no_std]

extern crate alloc;

pub mod emit;
pub mod error;
pub mod module;
pub mod options;

#[cfg(feature = "runtime")]
pub mod runtime;

pub use emit::emit_module;
pub use error::WasmError;
pub use module::{WasmExport, WasmModule};
pub use options::WasmOptions;

#[cfg(feature = "runtime")]
pub use runtime::{WasmEngine, WasmInstance};
```

**Placeholder files:**

- `emit.rs`: `pub fn emit_module(...) { todo!() }`
- `emit/mod.rs`: `pub use super::emit_module;`
- `emit/*.rs`: empty or minimal stubs
- `module.rs`: `pub struct WasmModule { todo!() }`
- `options.rs`: `pub struct WasmOptions { todo!() }`
- `error.rs`: `pub enum WasmError { todo!() }`
- `runtime/mod.rs`: `pub mod engine; pub mod instance;`
- `runtime/engine.rs`: `pub struct WasmEngine { todo!() }`
- `runtime/instance.rs`: `pub struct WasmInstance { todo!() }`

### Validate

```bash
cargo check -p lpvm-wasm --no-default-features
cargo check -p lpvm-wasm --features runtime
```

Both should compile (empty implementations are fine for this phase).

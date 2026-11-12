# Phase 3: Update LpvmEngine Trait

## Scope

Add `memory()` method to `LpvmEngine` trait. Update imports and re-exports
in `lib.rs`.

## Code Organization Reminders

- Keep trait definitions clean and well-documented
- Associated types come first, methods after
- Document the relationship between engine and memory

## Implementation Details

### Update: `lp-shader/lpvm/src/engine.rs`

```rust
//! `LpvmEngine` trait - backend configuration and compilation.

use lpir::module::IrModule;
use lps_shared::LpsModuleSig;

use crate::module::LpvmModule;
use crate::memory::LpvmMemory;  // NEW: import LpvmMemory

/// Backend engine that compiles LPIR modules and provides shared memory.
///
/// Implementations hold:
/// - Shared configuration and cached resources (e.g., wasmtime Engine)
/// - Shared memory allocator (`LpvmMemory` via `memory()`)
///
/// A single engine can compile multiple modules. All modules from the same
/// engine share the engine's memory (accessible via `memory()`). This enables
/// cross-shader data sharing (textures, globals).
///
/// # Shared Memory
///
/// The engine owns the shared memory allocator. External code allocates
/// shared data via `engine.memory().alloc(size)`. The returned `ShaderPtr`
/// has two representations:
/// - `native_ptr()` for host direct access (unsafe)
/// - `guest_value()` for shader access through uniforms
///
/// Modules receive a reference to the engine's memory at compile time.
/// Instances use this memory internally at instantiation time.
pub trait LpvmEngine {
    /// Compiled module type produced by this engine.
    type Module: LpvmModule;

    /// Error type for compilation failures.
    type Error: core::fmt::Display;

    /// Compile an LPIR module into a runnable module.
    ///
    /// The `meta` parameter provides the function signatures and other metadata
    /// needed for the compiled artifact. The module is associated with this
    /// engine's shared memory (accessible via `engine.memory()` from the module).
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;

    /// Get a reference to the engine's shared memory allocator.
    ///
    /// The returned `LpvmMemory` is used to allocate shared data (textures,
    /// globals) that can be accessed by both host code and shader instances.
    fn memory(&self) -> &dyn LpvmMemory;
}
```

### Update: `lp-shader/lpvm/src/lib.rs`

Add new module imports and re-exports:

```rust
//! LPVM runtime - traits, VM context, and execution abstractions.
//!
//! This crate provides the trait definitions for LPVM backends and runtime
//! data structures for shader execution. The core traits are:
//! - [`LpvmEngine`] - backend configuration, compilation, and shared memory
//! - [`LpvmModule`] - compiled artifact with metadata
//! - [`LpvmInstance`] - execution state and function calling
//! - [`LpvmMemory`] - shared memory allocator (NEW)
//!
//! Shared data (textures, globals) is allocated via [`LpvmEngine::memory()`],
//! producing a [`ShaderPtr`] with dual native/guest representations.

#![no_std]

extern crate alloc;

mod data;
mod data_error;
mod engine;
mod instance;
mod memory;              // NEW: LpvmMemory trait, AllocError
mod module;
mod shader_ptr;          // NEW: ShaderPtr type
mod vmcontext;

pub use data::LpvmData;
pub use data_error::DataError;
pub use engine::LpvmEngine;
pub use instance::LpvmInstance;
pub use memory::{AllocError, LpvmMemory};  // NEW: re-export memory types
pub use shader_ptr::ShaderPtr;             // NEW: re-export ShaderPtr
pub use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_shared::lps_value::LpsValue;
pub use lps_shared::path::{LpsPathSeg, PathParseError, parse_path};
pub use lps_shared::path_resolve::{LpsTypePathExt, PathError};
pub use lps_shared::value_path::{LpsValuePathError, LpsValuePathExt};
pub use lps_shared::{LayoutRules, LpsType, StructMember};
pub use module::LpvmModule;
pub use vmcontext::{
    DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA,
    VMCTX_OFFSET_TRAP_HANDLER, VmContext, VmContextHeader, minimal_vmcontext,
};
```

## Tests

Verify the trait coherence by creating a dummy implementation:

```rust
#[cfg(test)]
mod engine_tests {
    use super::*;
    use lpir::module::IrModule;
    use lps_shared::LpsModuleSig;

    struct DummyEngine;
    struct DummyModule;
    struct DummyInstance;

    impl LpvmMemory for DummyEngine {
        fn alloc(&self, _size: usize) -> Result<ShaderPtr, AllocError> {
            unimplemented!()
        }
        fn free(&self, _ptr: ShaderPtr) {}
        fn realloc(&self, _ptr: ShaderPtr, _new_size: usize) -> Result<ShaderPtr, AllocError> {
            unimplemented!()
        }
    }

    impl LpvmEngine for DummyEngine {
        type Module = DummyModule;
        type Error = core::fmt::Error;

        fn compile(&self, _ir: &IrModule, _meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
            unimplemented!()
        }

        fn memory(&self) -> &dyn LpvmMemory {
            self
        }
    }

    impl LpvmModule for DummyModule {
        type Instance = DummyInstance;
        type Error = core::fmt::Error;

        fn signatures(&self) -> &LpsModuleSig {
            unimplemented!()
        }

        fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
            unimplemented!()
        }
    }

    impl LpvmInstance for DummyInstance {
        type Error = core::fmt::Error;

        fn call(&mut self, _name: &str, _args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
            unimplemented!()
        }
    }
}
```

## Validate

```bash
cargo check -p lpvm
cargo test -p lpvm engine_tests
cargo +nightly fmt --check
```

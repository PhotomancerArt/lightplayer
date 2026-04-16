# Phase 4: Update VmContext Documentation

## Scope

Update documentation in `vmcontext.rs` to clarify that `VmContext` is
per-instance and does not hold shared memory references. Update module-level
docs in other files as needed.

## Code Organization Reminders

- Keep doc comments accurate and complete
- Use intra-doc links where helpful
- Update module-level documentation to reflect the architecture

## Implementation Details

### Update: `lp-shader/lpvm/src/vmcontext.rs`

Update the module-level doc and struct doc:

```rust
//! Fixed-layout header at the start of every VMContext allocation.
//!
//! On the reference embedded target (32-bit pointer), [`VmContext`] is 16 bytes. On 64-bit hosts
//! the `metadata` pointer is wider and the struct is larger; use [`core::mem::offset_of!`] (or the
//! `VMCTX_OFFSET_*` constants) instead of assuming a single cross-target size.
//!
//! # Per-Instance vs Shared Data
//!
//! `VmContext` is **per-instance** — each shader execution instance has its own
//! `VmContext` with independent:
//! - Instruction fuel (countdown to trap)
//! - Trap handler state
//! - Per-instance metadata pointer
//!
//! For **shared data** (textures, globals shared across shader instances), use
//! the engine's [`LpvmMemory`](crate::LpvmMemory) allocator:
//!
//! ```rust,ignore
//! let ptr = engine.memory().alloc(size)?;
//! // Host accesses via ptr.native_ptr() (unsafe)
//! // Shaders receive ptr.guest_value() through uniforms
//! ```
//!
//! The `VmContext` does not contain pointers to shared memory. Shaders access
//! shared data through uniform values (guest pointer representation).

use alloc::boxed::Box;

use crate::LpsValue;
use lps_shared::LpsType;

/// Default instruction fuel for new [`VmContext`] values (tests and host JIT calls).
pub const DEFAULT_VMCTX_FUEL: u64 = 1_000_000;

// ... constants remain unchanged ...

/// Well-known fields at the start of every VMContext (single flat allocation).
///
/// # Scope
///
/// `VmContext` is **per-instance**. Each shader execution instance has its own
/// context with:
/// - Fuel counter (instruction budget before trap)
/// - Trap handler state
/// - Metadata pointer (for per-instance globals/uniforms layout)
///
/// # Shared Data
///
/// Shared memory (textures, cross-shader globals) is **not** accessed through
/// `VmContext`. Instead:
/// - Host code allocates via [`LpvmEngine::memory()`](crate::LpvmEngine::memory)
/// - Host accesses data via [`ShaderPtr::native_ptr()`](crate::ShaderPtr::native_ptr) (unsafe)
/// - Shaders receive [`ShaderPtr::guest_value()`](crate::ShaderPtr::guest_value) through uniforms
/// - Shader `Load`/`Store` operations use the guest pointer value
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VmContext {
    pub fuel: u64,
    pub trap_handler: u32,
    /// Describes per-instance globals/uniforms layout; may be null until wired up.
    ///
    /// For shared (cross-instance) data, use uniforms with `ShaderPtr` guest values.
    pub metadata: *const LpsType,
}
```

### Update: `lp-shader/lpvm/src/engine.rs` module doc

Already updated in Phase 3. Ensure it mentions the shared memory model.

### Update: `lp-shader/lpvm/src/memory.rs` (already done in Phase 1)

Ensure documentation is complete with examples.

## Tests

Documentation tests (doctests) compile and run:

```bash
cargo test -p lpvm --doc
```

## Validate

```bash
cargo check -p lpvm
cargo test -p lpvm --doc
cargo doc -p lpvm --no-deps  # Verify docs build without warnings
```

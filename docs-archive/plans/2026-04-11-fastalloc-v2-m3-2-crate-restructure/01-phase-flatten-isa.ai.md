# Phase 1: Flatten `isa/rv32/` → `rv32/`

## Scope

Move files from `isa/rv32/` to `rv32/` at crate root. Delete `isa/mod.rs`
(dead `IsaBackend` trait, `Rv32Backend`, `CodeBlob`). Update all `crate::isa::`
import paths. No behavior changes.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Create `rv32/` module at crate root

Move these files from `isa/rv32/` → `rv32/`:
- `abi.rs`
- `alloc.rs`
- `encode.rs`
- `gpr.rs`
- `inst.rs`
- `rv32_emit.rs` (was `phys_emit.rs`, already renamed)
- `emit.rs` (the old monolith — kept temporarily, deleted in phase 2)
- `debug/mod.rs`, `debug/disasm.rs`, `debug/pinst.rs`

Create `rv32/mod.rs` based on the old `isa/rv32/mod.rs` but without the
`isa::` prefix in imports:

```rust
//! RV32 ISA-specific code: encoding, GPR, ABI, allocation, PInst emission.

pub mod abi;
pub mod alloc;
pub mod debug;
pub mod emit;
pub mod encode;
pub mod gpr;
pub mod inst;

// rv32_emit is included via #[path] in emit.rs for now
```

### 2. Delete `isa/` directory

- Delete `isa/mod.rs` entirely (contains dead `IsaBackend`, `Rv32Backend`,
  `CodeBlob`)
- Delete `isa/rv32/` directory (files moved)
- Remove `pub mod isa;` from `lib.rs`

### 3. Update `lib.rs`

Replace:
```rust
pub mod isa;
```
With:
```rust
pub mod rv32;
```

Update re-exports:
```rust
// Old:
pub use isa::rv32::emit_function_fastalloc_bytes;
pub use isa::{CodeBlob, IsaBackend, Rv32Backend};

// New:
pub use rv32::emit_function_fastalloc_bytes;
// Delete CodeBlob, IsaBackend, Rv32Backend re-exports
```

Also remove:
```rust
pub use regalloc::{Allocation, GreedyAlloc, LinearScan, RegAlloc, VRegInfo};
```
(regalloc module no longer exists)

### 4. Update all internal imports

Every file that uses `crate::isa::rv32::` must change to `crate::rv32::`.
Key files to update:

- `lower.rs`: `crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD` → `crate::rv32::abi::SRET_SCALAR_THRESHOLD`
- `rv32/emit.rs`: `super::` references stay the same (within rv32/)
- `debug_asm.rs`: `crate::isa::rv32::debug::` → `crate::rv32::debug::`
- `debug_asm.rs`: `crate::isa::rv32::emit::` → `crate::rv32::emit::`
- `rt_jit/compiler.rs`: `crate::isa::rv32::emit::` → `crate::rv32::emit::`
- `rt_emu/engine.rs`: `crate::isa::rv32::emit::` → `crate::rv32::emit::`

### 5. Update `rv32/emit.rs` internal imports

The old emit.rs uses `use crate::regalloc::{Allocation, GreedyAlloc, LinearScan, PReg};`.
Since `regalloc/` is deleted, this will break. Add a `// TODO: phase 2 deletes this file`
comment and stub out the missing imports temporarily if needed, or just let it
be dead code (it references deleted modules). If it won't compile, comment out
the `allocate_for_emit` function body and mark with TODO.

## Validate

```bash
# May not fully compile yet due to dead references in old emit.rs
# Goal: all non-emit.rs modules compile with new paths
cargo check -p lpvm-native 2>&1 | head -60
```

Fix all import path errors. The old `rv32/emit.rs` may have broken references
to `crate::regalloc` — that's OK, it gets deleted in phase 2.

# Phase 4: Create root `emit.rs`

## Scope

Create the shared emission module at `src/emit.rs`. This orchestrates
VInst + Allocation → bytes + relocs, calling into `rv32/rv32_emit.rs` for
ISA-specific encoding. This replaces the emission logic that was in the old
`isa/rv32/emit.rs`.

**Note:** In the current fastalloc pipeline, the allocator (`rv32/alloc.rs`)
already produces `Vec<PInst>` and the `Rv32Emitter` already converts PInst →
bytes. So `emit.rs` at this stage is thin — it provides the entry point that
`compile.rs` calls, handling the conversion from `PhysReloc` to `NativeReloc`
and any debug line mapping.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Create `src/emit.rs`

```rust
//! Shared emission: PInst sequence → machine code bytes + relocations.
//!
//! Calls into [`crate::rv32::rv32_emit::Rv32Emitter`] for ISA-specific encoding.

use alloc::vec::Vec;

use crate::compile::NativeReloc;
use crate::rv32::inst::PInst;
use crate::rv32::rv32_emit::Rv32Emitter;

/// Emitted code for one function.
pub struct EmittedCode {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
}

/// Emit a sequence of PInsts to machine code bytes.
pub fn emit_pinsts(pinsts: &[PInst]) -> EmittedCode {
    let mut emitter = Rv32Emitter::new();
    for p in pinsts {
        emitter.emit(p);
    }
    let (code, phys_relocs) = emitter.finish_with_relocs();
    let relocs = phys_relocs
        .into_iter()
        .map(|r| NativeReloc {
            offset: r.offset,
            symbol: r.symbol,
        })
        .collect();
    EmittedCode { code, relocs }
}
```

### 2. Update `compile.rs` to use `emit.rs`

Replace the inline emission logic in `compile_function` with a call to
`emit::emit_pinsts`:

```rust
// In compile_function, replace:
//   let mut emitter = crate::rv32::rv32_emit::Rv32Emitter::new();
//   ...
// With:
    let emitted = crate::emit::emit_pinsts(&pinsts);
```

### 3. Add `pub mod emit;` to `lib.rs`

## Validate

```bash
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa -- rv32::alloc::tests
cargo test -p lpvm-native-fa -- rv32::rv32_emit::tests
```

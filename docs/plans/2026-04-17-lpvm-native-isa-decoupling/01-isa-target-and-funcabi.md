# Phase 1: IsaTarget + FuncAbi Plumbing

## Goal

Introduce the `IsaTarget` enum and plumb it through the public construction
surfaces of `FuncAbi`, `ModuleAbi`, the JIT runtime, and link entry points.
Add the per-target methods that later phases will call. **No call site
changes behavior** — this phase is strictly additive scaffolding.

At end of phase: `IsaTarget::Rv32imac` is threaded end-to-end. Existing code
that bypasses it (e.g. `regalloc/walk.rs` reaching into
`crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD`) is untouched and still
works. Phases 2-4 cut those bypasses one layer at a time.

## Steps

### 1.1 Define `IsaTarget`

Add to `lp-shader/lpvm-native/src/isa/mod.rs`:

```rust
/// The target ISA + sub-architecture for a compiled module.
///
/// Variant names describe the **target hardware**, not the codegen output.
/// `Rv32imac` is the ESP32-C6 target (`riscv32imac-unknown-none-elf`); the
/// emitter currently produces only base RV32IM instructions. The A and C
/// extensions appear in the target name because the firmware runtime uses
/// them, not because we emit them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IsaTarget {
    Rv32imac,
}
```

Re-export from `lib.rs`:
```rust
pub use isa::IsaTarget;
```

### 1.2 Add per-target methods on `IsaTarget`

In `isa/mod.rs`, add an `impl` block. Each method is a `match` that
delegates to `crate::isa::rv32::*` constants/functions for the
`Rv32imac` arm. **Do not change** the rv32 constants/functions yet —
just expose them through `IsaTarget`.

Methods to add (signatures may need small adjustments to match existing
RV32 types):

```rust
impl IsaTarget {
    /// Pool-init order for the register allocator's LRU.
    pub fn allocatable_pool_order(self) -> &'static [u8];

    /// True if `p` is in the allocatable register pool.
    pub fn is_in_allocatable_pool(self, p: u8) -> bool;

    /// Human-readable name for `p` (debug rendering only).
    pub fn reg_name(self, p: u8) -> &'static str;

    /// True if a return value with `scalar_count` scalars uses the
    /// sret-via-buffer convention rather than direct registers.
    pub fn sret_uses_buffer_for(self, scalar_count: u32) -> bool;

    /// Minimum stack frame alignment in bytes.
    pub fn stack_alignment(self) -> u32;

    /// `object` crate Architecture for ELF emission.
    pub fn elf_architecture(self) -> object::Architecture;

    /// e_flags value for ELF header.
    pub fn elf_e_flags(self) -> u32;
}
```

For `Rv32imac`, the bodies should reference:
- `allocatable_pool_order` → `crate::isa::rv32::gpr::ALLOC_POOL`
- `is_in_allocatable_pool` → `crate::isa::rv32::gpr::is_in_alloc_pool` (or
  derive from `ALLOC_POOL.contains`)
- `reg_name` → `crate::isa::rv32::gpr::reg_name`
- `sret_uses_buffer_for` → `n > crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD`
- `stack_alignment` → 16
- `elf_architecture` → `object::Architecture::Riscv32`
- `elf_e_flags` → `EF_RISCV_FLOAT_ABI_SOFT` (the same constant `link.rs`
  uses today)

If the underlying rv32 type for `p` is `crate::isa::rv32::gpr::PReg` (which
is `u8`), the `IsaTarget` methods accept `u8` to keep the signature
ISA-neutral. Internal conversion is trivial since the alias is `u8`.

### 1.3 Add `isa: IsaTarget` field to `FuncAbi`

In `lp-shader/lpvm-native/src/abi/func_abi.rs`:

```rust
pub struct FuncAbi {
    // ... existing fields ...
    pub(crate) isa: IsaTarget,
}

impl FuncAbi {
    pub fn isa(&self) -> IsaTarget { self.isa }

    pub fn stack_alignment(&self) -> u32 {
        self.isa.stack_alignment()
    }
}
```

`func_abi_rv32` (the constructor in `crate::isa::rv32::abi`) sets
`isa: IsaTarget::Rv32imac` on the returned `FuncAbi`. No other constructor
exists today.

### 1.4 Add Category-1 accessors on `FuncAbi`

Two new methods used by phase 2:

```rust
impl FuncAbi {
    /// Argument-passing registers, in order.
    pub fn arg_regs(&self) -> &[PReg] {
        // For Rv32imac, this is crate::isa::rv32::abi::ARG_REGS.
        // Phase 1: dispatch via match on self.isa.
    }

    /// True if `p` is caller-saved (i.e. clobbered across calls)
    /// per this function's ABI.
    pub fn is_caller_saved_pool(&self, p: PReg) -> bool {
        // Derive from self.call_clobbers (already on FuncAbi).
    }
}
```

`PReg` here is `crate::abi::PReg` (the canonical 2-byte type). The body of
`arg_regs` will need to convert from the rv32 `u8` array — fine to do this
once at access time, or to materialize a `&'static [PReg]` constant.

### 1.5 Plumb `IsaTarget` through `ModuleAbi`

In `abi/func_abi.rs`:

```rust
impl ModuleAbi {
    pub fn from_ir_and_sig(
        isa: IsaTarget,
        // ... existing args ...
    ) -> Self {
        // ... existing body, but dispatch FuncAbi construction:
        let func_abi = match isa {
            IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(...),
        };
        // ...
    }
}
```

Update all callers to pass `IsaTarget::Rv32imac`. Audit:
- `lp-shader/lpvm-native/src/compile.rs:89`
- `lp-shader/lpvm-native/src/rt_jit/module.rs:64`

### 1.6 Plumb `IsaTarget` through link entry points

In `lp-shader/lpvm-native/src/link.rs`:
- `link_jit(...)` gains `isa: IsaTarget` parameter (unused this phase
  except passed through; phase 4 makes it active).
- `link_elf(...)` gains `isa: IsaTarget` parameter (unused this phase).

Update all callers. Phase 4 will actually use these parameters.

### 1.7 Plumb `IsaTarget` through JIT runtime

In `rt_jit/module.rs` and `rt_jit/instance.rs`:
- `NativeJitModule::compile(...)` (or equivalent entry) takes `IsaTarget`.
- Wire down to `ModuleAbi::from_ir_and_sig` and `link_jit`.
- For now, all upstream callers hardcode `IsaTarget::Rv32imac`.

### 1.8 Verify

```
cargo check -p lpvm-native
cargo test -p lpvm-native
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

All must pass with no behavior change.

## Validation

- `cargo check -p lpvm-native` clean
- `cargo test -p lpvm-native` all green
- ESP32 target check clean
- `IsaTarget::Rv32imac` is reachable from `lp-shader/lpvm-native/src/lib.rs`
- `FuncAbi::isa()`, `FuncAbi::arg_regs()`, `FuncAbi::is_caller_saved_pool()`
  all compile and return correct values for RV32
- `ModuleAbi::from_ir_and_sig` takes `IsaTarget` and dispatches via `match`
- `link_jit` and `link_elf` accept `IsaTarget` (even if unused this phase)
- Existing leakage (`regalloc → crate::isa::rv32`, etc.) is **still
  present** — phase 2+ removes it

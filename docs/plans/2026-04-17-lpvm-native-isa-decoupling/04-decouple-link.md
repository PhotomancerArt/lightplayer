# Phase 4: Decouple link.rs

## Goal

Split `lp-shader/lpvm-native/src/link.rs` into:

1. **`link.rs`** — generic relocation orchestration (no instruction bytes,
   no architecture constants).
2. **`crate::isa::rv32::link`** (new module) — RV32 instruction encoding
   for call patches, RV32 relocation r_type constants, RV32 ELF metadata.

After this phase, `link.rs` dispatches per-relocation patching via a small
`match` on `IsaTarget`. `link_elf` derives `Architecture` and `e_flags`
via `IsaTarget` methods.

This phase relies on Phase 1 having added the `IsaTarget::elf_architecture`
and `IsaTarget::elf_e_flags` methods (delegating to current hardcoded
constants for `Rv32imac`).

## Inventory

| Concern                            | Today (in `link.rs`)                       | After                                         |
| ---------------------------------- | ------------------------------------------ | --------------------------------------------- |
| Call patch instruction emission    | `patch_call_plt(...)` (auipc + jalr)       | `crate::isa::rv32::link::patch_call_plt`      |
| Relocation r_type constant         | `R_RISCV_CALL_PLT = 17`                    | `crate::isa::rv32::link::R_RISCV_CALL_PLT`    |
| ELF architecture                   | `Architecture::Riscv32` (hardcoded)        | `isa.elf_architecture()`                      |
| ELF e_flags                        | `EF_RISCV_FLOAT_ABI_SOFT` (hardcoded)      | `isa.elf_e_flags()`                           |
| Per-relocation dispatch            | direct call to `patch_call_plt`            | `match isa { IsaTarget::Rv32imac => ... }`    |

## Steps

### 4.1 Create `crate::isa::rv32::link`

New file `lp-shader/lpvm-native/src/isa/rv32/link.rs`:

```rust
//! RV32-specific linker helpers: call-site patching and ELF metadata.

use crate::link::{NativeReloc, PatchError};

pub const R_RISCV_CALL_PLT: u32 = 17;

/// e_flags value for the soft-float ABI used by ESP32-C6.
pub const EF_RISCV_FLOAT_ABI_SOFT: u32 = 0x0;

/// Patch an RV32 `auipc + jalr` call sequence at `code[reloc.offset..]` so
/// the call resolves to `target_addr`.
pub fn patch_call_plt(
    code: &mut [u8],
    reloc: &NativeReloc,
    target_addr: u64,
) -> Result<(), PatchError> {
    // Move the body of the current `link.rs::patch_call_plt` here verbatim.
    // No behavior change.
    todo!()
}
```

Add `pub mod link;` to `lp-shader/lpvm-native/src/isa/rv32/mod.rs`.

### 4.2 Move `patch_call_plt` body

Cut the body from `link.rs::patch_call_plt` and paste into the new
`isa::rv32::link::patch_call_plt`. Imports it needs (`NativeReloc`, etc.)
come from `crate::link`. Delete the old `patch_call_plt` from `link.rs`.

Move `R_RISCV_CALL_PLT` and `EF_RISCV_FLOAT_ABI_SOFT` constants from
`link.rs` to the new module.

### 4.3 Update `IsaTarget` ELF methods

In `isa/mod.rs`, the methods added in Phase 1 should now delegate to the
new module:

```rust
impl IsaTarget {
    pub fn elf_architecture(self) -> object::Architecture {
        match self {
            IsaTarget::Rv32imac => object::Architecture::Riscv32,
        }
    }

    pub fn elf_e_flags(self) -> u32 {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::link::EF_RISCV_FLOAT_ABI_SOFT,
        }
    }
}
```

(Phase 1 may have inlined these constants directly in the method; this
phase just points them at the per-ISA module.)

### 4.4 Refactor `link_jit` for dispatched patching

`link_jit` walks relocations and calls `patch_call_plt` for each. Refactor
to dispatch on `IsaTarget`:

```rust
pub fn link_jit(
    isa: IsaTarget,
    code: &mut [u8],
    relocs: &[NativeReloc],
    symbols: &SymbolMap,
) -> Result<(), LinkError> {
    for reloc in relocs {
        let target_addr = symbols.resolve(reloc.symbol)?;
        match isa {
            IsaTarget::Rv32imac => {
                if reloc.r_type == crate::isa::rv32::link::R_RISCV_CALL_PLT {
                    crate::isa::rv32::link::patch_call_plt(code, reloc, target_addr)?;
                } else {
                    return Err(LinkError::UnsupportedReloc {
                        isa,
                        r_type: reloc.r_type,
                    });
                }
            }
        }
    }
    Ok(())
}
```

If `NativeReloc` doesn't currently carry `r_type` explicitly (because
there's only one), add it now — even RV32 will eventually need this for
PC-relative loads etc., and it's needed for ARM at all.

### 4.5 Refactor `link_elf` to use IsaTarget metadata

```rust
pub fn link_elf(
    isa: IsaTarget,
    // ... existing args ...
) -> Result<Vec<u8>, LinkError> {
    let arch = isa.elf_architecture();
    let e_flags = isa.elf_e_flags();
    // ... existing body, with `arch` and `e_flags` substituted for the
    //     hardcoded `Architecture::Riscv32` / `EF_RISCV_FLOAT_ABI_SOFT` ...
}
```

### 4.6 Audit callers of `link_jit` / `link_elf`

`IsaTarget` was added to these signatures in Phase 1.6 (parameter
threaded but unused). This phase makes the parameter active. No new
caller-side changes — they're already passing `IsaTarget::Rv32imac`.

### 4.7 Verify

```
rg 'auipc|jalr|R_RISCV|Riscv32|EF_RISCV' lp-shader/lpvm-native/src/link.rs
# Should produce ZERO matches. All RV32-flavored content moved.

rg 'use crate::isa::rv32' lp-shader/lpvm-native/src/link.rs
# Allowed: link.rs may import the rv32 link module via the dispatch path;
# the goal is no instruction bytes / arch constants in link.rs itself.
# (If you can keep this empty by routing entirely through IsaTarget
# methods, even better.)

cargo check -p lpvm-native
cargo test -p lpvm-native
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Validation

- `link.rs` contains no RV32 instruction bytes or RV32 r_type constants
- `crate::isa::rv32::link` exists and contains `patch_call_plt`,
  `R_RISCV_CALL_PLT`, `EF_RISCV_FLOAT_ABI_SOFT`
- `link_jit` dispatches via `match isa`
- `link_elf` derives `Architecture` and `e_flags` from `IsaTarget` methods
- `cargo check -p lpvm-native` clean
- `cargo test -p lpvm-native` all green
- ESP32 firmware compiles and a smoke test still produces a working JIT
  output (a unit test that runs `link_jit` with `IsaTarget::Rv32imac` and
  checks the patched bytes is the easiest regression guard)

## Notes

- The dispatch via `match` inside `link_jit` looks slightly redundant when
  there's only one variant. That's fine — it's the explicit hook point
  where the second ISA slots in. Compiler will warn if a new variant is
  added without an arm here, which is exactly what we want.
- If `NativeReloc` doesn't carry an `r_type` field today, this phase adds
  one. RV32 relocations all become `R_RISCV_CALL_PLT` for now; ARM will
  introduce `R_ARM_CALL` etc. in its own time.
- Do **not** introduce a `LinkBackend` trait or vtable. `match` is the
  right shape per `00-notes.md` Q3.

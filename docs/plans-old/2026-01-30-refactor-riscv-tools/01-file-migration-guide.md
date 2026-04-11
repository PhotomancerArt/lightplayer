# File Migration Guide

This document lists exactly which files need to be moved where.

## Files to Move to `lp-riscv-inst/`

**From:** `lp-riscv/lp-riscv-tools/src/`

**To:** `lp-riscv/lp-riscv-inst/src/`

- `auipc_imm.rs`
- `decode.rs`
- `decode_rvc.rs`
- `encode.rs`
- `format.rs`
- `inst.rs`
- `register_role.rs`
- `regs.rs`

**Note:** `debug.rs` is already created in `lp-riscv-inst/src/debug.rs` - you can delete the old one from `lp-riscv-tools/src/debug.rs` after moving.

## Files to Move to `lp-riscv-emu/`

**From:** `lp-riscv/lp-riscv-tools/src/`

**To:** `lp-riscv/lp-riscv-emu/src/`

- `emu/` (entire directory)
- `serial/` (entire directory)

**Note:** `lib.rs` is already created in `lp-riscv-emu/src/lib.rs` - you can delete the old one from `lp-riscv-tools/src/lib.rs` after moving.

## Files to Move to `lp-riscv-elf/`

**From:** `lp-riscv/lp-riscv-tools/src/`

**To:** `lp-riscv/lp-riscv-elf/src/`

- `elf_loader/` (entire directory)
- `elf_linker.rs`

**Note:** `lib.rs` is already created in `lp-riscv-elf/src/lib.rs` - you can delete the old one from `lp-riscv-tools/src/lib.rs` after moving.

## Tests to Move

### Tests for `lp-riscv-inst`

**From:** `lp-riscv/lp-riscv-tools/tests/`

**To:** `lp-riscv/lp-riscv-inst/tests/`

- `instruction_tests.rs`

### Tests for `lp-riscv-emu`

**From:** `lp-riscv/lp-riscv-tools/tests/`

**To:** `lp-riscv/lp-riscv-emu/tests/`

- `abi_tests.rs`
- `stack_args_tests.rs`
- `multi_return_test.rs`
- `trap_tests.rs`
- `riscv_nostd_test.rs`

### Tests for `lp-riscv-elf`

**From:** `lp-riscv/lp-riscv-tools/tests/`

**To:** `lp-riscv/lp-riscv-elf/tests/`

- `elf_loader_test.rs`
- `guest_app_tests.rs`

## Examples to Move

**From:** `lp-riscv/lp-riscv-tools/examples/`

**To:** `lp-riscv/lp-riscv-elf/examples/` (since it uses ELF loading)

- `simple_codegen.rs`

## Files to Update After Moving

After moving files, these files will need import path updates:

1. **`lp-riscv-emu/src/emu/`** - Update imports from `crate::` to `lp_riscv_inst::` for instruction utilities
2. **`lp-riscv-elf/src/elf_loader/`** - Update imports from `crate::` to `lp_riscv_inst::` and `lp_riscv_emu::`
3. **All test files** - Update imports to use new crate names

## Summary

```
lp-riscv-inst/src/
  ├── lib.rs (already created)
  ├── debug.rs (already created)
  ├── auipc_imm.rs (MOVE)
  ├── decode.rs (MOVE)
  ├── decode_rvc.rs (MOVE)
  ├── encode.rs (MOVE)
  ├── format.rs (MOVE)
  ├── inst.rs (MOVE)
  ├── register_role.rs (MOVE)
  └── regs.rs (MOVE)

lp-riscv-emu/src/
  ├── lib.rs (already created)
  ├── emu/ (MOVE entire directory)
  └── serial/ (MOVE entire directory)

lp-riscv-elf/src/
  ├── lib.rs (already created)
  ├── elf_loader/ (MOVE entire directory)
  └── elf_linker.rs (MOVE)
```

# Code Updates Needed After File Migration

After you move the files, these code files will need import path updates.

## Files in `lp-riscv-emu/` that need updates

### `lp-riscv-emu/src/emu/executor.rs`

- Change `use crate::{Gpr, Inst}` to `use lp_riscv_inst::{Gpr, Inst}`
- Change `crate::inst::format_instruction` to `lp_riscv_inst::format_instruction`

### `lp-riscv-emu/src/emu/emulator/execution.rs`

- Change `use crate::{Gpr, Inst}` to `use lp_riscv_inst::{Gpr, Inst}`
- Change `crate::inst::format_instruction` to `lp_riscv_inst::format_instruction`

### `lp-riscv-emu/src/emu/emulator/debug.rs`

- Change `use crate::{Gpr, Inst}` to `use lp_riscv_inst::{Gpr, Inst}`
- Change `crate::inst::format_instruction` to `lp_riscv_inst::format_instruction`

### `lp-riscv-emu/src/emu/emulator/function_call.rs`

- Change `use crate::Gpr` to `use lp_riscv_inst::Gpr`

### `lp-riscv-emu/src/emu/emulator/registers.rs`

- Change `use crate::Gpr` to `use lp_riscv_inst::Gpr`

### `lp-riscv-emu/src/emu/decoder.rs`

- Change `use crate::decode::decode_instruction` to `use lp_riscv_inst::decode_instruction`
- Change `use crate::Inst` to `use lp_riscv_inst::Inst`

### `lp-riscv-emu/src/emu/logging.rs`

- Change `use crate::Gpr` to `use lp_riscv_inst::Gpr`
- Change `crate::inst::format_instruction` to `lp_riscv_inst::format_instruction`

### `lp-riscv-emu/src/emu/abi_helper.rs`

- No changes needed (doesn't use instruction utilities directly)

## Files in `lp-riscv-elf/` that need updates

### `lp-riscv-elf/src/elf_loader/mod.rs`

- Change `use crate::emu::{LogLevel, Riscv32Emulator, StepResult}` to
  `use lp_riscv_emu::{LogLevel, Riscv32Emulator, StepResult}`
- Change `crate::debug!` to `lp_riscv_inst::debug!` (or just `debug!` if re-exported)

### `lp-riscv-elf/src/elf_loader/object/tests.rs`

- Change `use crate::emu::{LogLevel, Riscv32Emulator, StepResult}` to
  `use lp_riscv_emu::{LogLevel, Riscv32Emulator, StepResult}`
- Change `use crate::Gpr` to `use lp_riscv_inst::Gpr`

### `lp-riscv-elf/src/elf_linker.rs`

- No changes needed (doesn't use emulator or instruction utilities)

## Test files that need updates

### `lp-riscv-inst/tests/instruction_tests.rs`

- Change `use lp_riscv_tools::*` to `use lp_riscv_inst::*`
- Change `use lp_riscv_tools::Riscv32Emulator` to `use lp_riscv_emu::Riscv32Emulator`

### `lp-riscv-emu/tests/*.rs`

- Change `use lp_riscv_tools::*` to appropriate imports:
  - `use lp_riscv_emu::*` for emulator types
  - `use lp_riscv_inst::*` for instruction utilities

### `lp-riscv-elf/tests/elf_loader_test.rs`

- Change `use lp_riscv_tools::*` to:
  - `use lp_riscv_elf::*` for ELF loading
  - `use lp_riscv_emu::*` for emulator types
  - `use lp_riscv_inst::*` for instruction utilities

### `lp-riscv-elf/tests/guest_app_tests.rs`

- Change `use lp_riscv_tools::{LogLevel, emu::Riscv32Emulator, load_elf}` to:
  - `use lp_riscv_emu::{LogLevel, Riscv32Emulator}`
  - `use lp_riscv_elf::load_elf`

## Example files that need updates

### `lp-riscv-elf/examples/simple_codegen.rs`

- Change `use lp_riscv_tools::{Gpr, Riscv32Emulator}` to:
  - `use lp_riscv_inst::Gpr`
  - `use lp_riscv_emu::Riscv32Emulator`

## Consumer code that needs updates

### `lp-glsl/lp-glsl-compiler/src/exec/emu.rs`

- Change `use lp_riscv_tools::emu::error::{EmulatorError, trap_code_to_string}` to
  `use lp_riscv_emu::EmulatorError`
- Change `lp_riscv_tools::emu::emulator::Riscv32Emulator` to `lp_riscv_emu::Riscv32Emulator`
- Change `lp_riscv_tools::format_instruction` to `lp_riscv_inst::format_instruction`

### `lp-glsl/lp-glsl-compiler/src/backend/codegen/emu.rs`

- Change `use lp_riscv_tools::Gpr` to `use lp_riscv_inst::Gpr`
- Change `use lp_riscv_tools::StepResult` to `use lp_riscv_emu::StepResult`
- Change `use lp_riscv_tools::elf_loader::load_elf` to `use lp_riscv_elf::load_elf`
- Change `use lp_riscv_tools::emu::LogLevel` to `use lp_riscv_emu::LogLevel`
- Change `use lp_riscv_tools::emu::emulator::Riscv32Emulator` to `use lp_riscv_emu::Riscv32Emulator`

### `lp-glsl/lp-glsl-compiler/src/backend/codegen/builtins_linker.rs`

- Change `lp_riscv_tools::ElfLoadInfo` to `lp_riscv_elf::ElfLoadInfo`
- Change `lp_riscv_tools::load_elf` to `lp_riscv_elf::load_elf`
- Change `lp_riscv_tools::elf_loader::load_object_file` to `lp_riscv_elf::load_object_file`

## Summary of import changes

| Old Import                           | New Import                          |
| ------------------------------------ | ----------------------------------- |
| `lp_riscv_tools::Gpr`                | `lp_riscv_inst::Gpr`                |
| `lp_riscv_tools::Inst`               | `lp_riscv_inst::Inst`               |
| `lp_riscv_tools::decode_instruction` | `lp_riscv_inst::decode_instruction` |
| `lp_riscv_tools::format_instruction` | `lp_riscv_inst::format_instruction` |
| `lp_riscv_tools::encode`             | `lp_riscv_inst::encode`             |
| `lp_riscv_tools::Riscv32Emulator`    | `lp_riscv_emu::Riscv32Emulator`     |
| `lp_riscv_tools::StepResult`         | `lp_riscv_emu::StepResult`          |
| `lp_riscv_tools::LogLevel`           | `lp_riscv_emu::LogLevel`            |
| `lp_riscv_tools::EmulatorError`      | `lp_riscv_emu::EmulatorError`       |
| `lp_riscv_tools::load_elf`           | `lp_riscv_elf::load_elf`            |
| `lp_riscv_tools::ElfLoadInfo`        | `lp_riscv_elf::ElfLoadInfo`         |
| `lp_riscv_tools::elf_loader::*`      | `lp_riscv_elf::*`                   |
| `lp_riscv_tools::emu::*`             | `lp_riscv_emu::*`                   |

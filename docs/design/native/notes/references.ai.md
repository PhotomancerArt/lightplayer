# LPVM-Native References

Key reference materials for implementing the `lpvm-native` custom backend.

## RISC-V Instruction Encoding

### Cranelift Fork (Local)

Our fork at `/Users/yona/dev/photomancer/lp-cranelift` contains proven RISC-V instruction encoders:

- **`cranelift/codegen/src/isa/riscv32/inst/encode.rs`** — R/I/S/B/U/J format encoding functions
  - Pure functions, no ISLE dependencies for core encodings
  - `encode_r_type_bits`, `encode_i_type`, `encode_s_type`, etc.
  - Adapt for our VInst → machine code pipeline

- **`cranelift/codegen/meta/src/isa/riscv32.rs`** — ISA extension flags (M, A, C, Zicsr, etc.)

### Cranelift Relocation Handling

- **`cranelift/jit/src/compiled_blob.rs`** — JIT relocation patching at load time
  - `perform_relocations()` resolves symbols and patches instruction bytes
  - `RiscvCallPlt` handling for auipc+jalr pairs

- **`cranelift/object/src/backend.rs`** — ELF relocation generation
  - Converts internal `ModuleReloc` to `object::Relocation` for ELF output
  - Shows pattern for dual JIT/ELF support

## QBE (Local)

### `/Users/yona/dev/photomancer/oss/qbe` and `/Users/yona/dev/photomancer/oss/qbe_riscv32_64`

Lightweight compiler backend structure reference:

- **`rv64/emit.c`** — Instruction emission, stack frame layout
  - `emitins()` switch over opcodes
  - `slot()` for stack slot addressing
  - Frame layout: `[saved ra] [saved fp] [spill slots] [locals] [callee-saves]`
  - Prologue/epilogue emission pattern

- **`rv64/abi.c`** — Register assignment, calling convention
- **`emit.c`** (top-level) — Text emission utilities, symbol handling

## RISC-V psABI Documentation

### `/Users/yona/dev/photomancer/oss/riscv-elf-psabi-doc`

Official RISC-V ELF psABI specification:

- **`riscv-elf.adoc`** — Core ELF and relocation definitions
  - § Relocations: `R_RISCV_CALL_PLT`, `R_RISCV_JAL`, `R_RISCV_PCREL_HI20/LO12_I`
  - § Procedure Call Linkage: auipc+jalr sequence and linker relaxation
  - § Integer Calling Convention: register roles, argument passing

Key relocation types for builtin calls:
- `R_RISCV_CALL_PLT` — auipc+jalr pair for 32-bit PC-relative calls
- `R_RISCV_JAL` — 20-bit PC-relative jump (simpler, ±1MB range)

## `object` crate (ELF writer, no_std)

`lpvm-native` uses:

```toml
object = { version = "0.37", default-features = false, features = ["write_core", "elf"] }
```

`write_core` alone does not enable ELF; add the `elf` feature. This keeps `std` off while writing relocatable `.o` files.

## External References

- **RISC-V Spec v2.2**: https://riscv.org/wp-content/uploads/2017/05/riscv-spec-v2.2.pdf
  - Chapter 2: Instruction formats (R/I/S/B/U/J types)
  - Chapter 25: Compressed instructions (if implementing C extension)

- **RISC-V psABI Doc (GitHub)**: https://github.com/riscv-non-isa/riscv-elf-psabi-doc
  - Official source for the local copy above

## Using These References

1. **Instruction encoding**: Start with Cranelift `encode.rs`, simplify for our subset
2. **Relocation design**: Follow Cranelift's `ModuleReloc` pattern for JIT/ELF dual support
3. **Frame layout**: Reference QBE `rv64/emit.c` structure, adapt to RV32 ILP32
4. **ABI compliance**: Cross-check with psABI doc for calling convention details

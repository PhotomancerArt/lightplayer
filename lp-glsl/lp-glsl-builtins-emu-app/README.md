# lp-glsl-builtins-emu-app

RISC-V32 (`riscv32imac-unknown-none-elf`) binary that **links every `lp-glsl-builtins` symbol** so
the **RV32 filetest** path (`lp-glsl-filetests`, `lp-riscv-emu`) can load a guest with a stable
builtin table. It is not the on-ESP32 firmware; it is the dedicated **builtin guest** for emulator
tests.

## What it does

- Links all `__lp_q32_*` (and related) builtins from `lp-glsl-builtins`
- Supplies the guest entry (`_entry`), `.bss` / `.data` style startup, and panic reporting for the
  emulator host
- Pulls in **generated** `builtin_refs.rs` so the linker does not drop unused builtins

## Build

From repo root:

```bash
scripts/build-builtins.sh
```

or (codegen optional if sources unchanged):

```bash
just build-rv32-builtins
```

Typical artifact:

`target/riscv32imac-unknown-none-elf/release/lp-glsl-builtins-emu-app`

`build-builtins.sh` also refreshes generated files when `lp-glsl-builtins/src/builtins/` or
`lp-glsl-builtins-gen-app/` change, then builds this crate and `lp-glsl-builtins-wasm`.

## Target flags (release guest)

The script uses a conservative RISC-V release profile (`opt-level=1`, `panic=abort`, single codegen
unit, etc.) tuned for a small, linkable guest — see `scripts/build-builtins.sh` for exact
`RUSTFLAGS`.

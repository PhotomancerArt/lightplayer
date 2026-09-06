# LightPlayer RISC-V 32-bit utilities

The low-level RISC-V 32 architecture crates the rest of LightPlayer builds
on: the instruction model and the ELF loader that both the shader compiler's
rv32 backend (`lpvm-native`) and the emulator use.

- **`lp-riscv-inst`** — RISC-V instruction encoding/decoding.
- **`lp-riscv-elf`** — ELF loading/linking (symbols, relocations, GOT) for
  JIT-compiled guest code.
- **`lp-riscv-tools`** — deprecated umbrella crate; use the crates above.

## What moved

The rv32 **emulator** crates — `lp-riscv-emu`, `lp-riscv-emu-guest`,
`lp-riscv-emu-guest-test-app` — now live in `lp-emu/`, with the rest of the
emulation family, under its MIT fence. Crate names did not change. The
arch-neutral machinery they build on (`lp-emu-core`, `lp-emu-abi`) has always
been there. See `lp-emu/README.md` and
`docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md`.

The crates in this directory stayed: they are compiler-backend
infrastructure, not emulator infrastructure, and they are AGPL like the rest
of the product (vision Q3).

The host↔guest protocol crate formerly here (`lp-riscv-emu-shared`) is now
`lp-emu/lp-emu-abi`; see `docs/adr/2026-07-28-emu-core-crate-family.md`.

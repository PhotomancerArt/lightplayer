# LightPlayer Xtensa utilities

This directory contains the low-level utilities for the Xtensa architecture
(ESP32-S3 / LX7 and classic ESP32 / LX6 — the two ISAs are identical for the
integer subset LightPlayer uses). It mirrors `lp-riscv/`'s role for rv32:
instruction model, emulator, and ELF loading that the shader compiler's
`isa/xt` backend and the Xtensa firmware crates build on.

- **`lp-xt-inst`** — Xtensa instruction model, encoder, variable-length
  decoder, and objdump-style disassembler (integer subset). Everything else
  sits on this crate.
- **`lp-xt-emu`** *(lands with M2 of the backport roadmap)* — the Xtensa
  emulator: windowed-register machinery, per-board memory maps
  (`BoardProfile`), built on `lp-emu/lp-emu-core`.
- **`lp-xt-elf`** *(lands with M2 — its loader/host API depends on
  `lp-xt-emu`)* — linked-ELF loader + guest-syscall host, plus a feature-gated
  relocatable-object engine (`R_XTENSA_32` / `R_XTENSA_SLOT0_OP`).
- **`lp-xt-emu-guest`** *(lands with M2)* — guest-side runtime for code
  running inside the emulator.

Naming rule (settled in the standalone-core plan): `lp-xt-*` crates are
product code; experiment-only tooling (payload runners, test rigs) stays in
the experiment repo.

## Provenance

These crates were built and hardware-verified in the public
[2026-esp32s3-experiment](https://github.com/PhotomancerArt/2026-esp32s3-experiment)
repo and landed here per its `docs/BACKPORT.md`. Instruction encoding data is
derived from `espressif/llvm-project` TableGen sources
(Apache-2.0 WITH LLVM-exception — text vendored at
`licenses/LLVM-Apache-2.0-with-LLVM-exception.txt`); each derived file carries
a provenance header. No GPL source was copied or transliterated; see
`docs/adr/2026-07-29-license-provenance-discipline.md`.

# LightPlayer RISC-V 32-bit utilities

This directory contains the low-level utilities for the RISC-V 32-bit architecture that are
used by the rest of LightPlayer, mostly for testing.

The main feature is a riscv32 emulator optimized for debugging and testing.

- **`lp-riscv-emu`** — the RISC-V 32 emulator: instruction executors, register
  file, run loops, `EmulatorError`, and the rv32 frame-pointer backtrace walk.
  The arch-neutral machinery it builds on (memory model, `StepResult`/`TrapCode`,
  serial, time, cycle accounting, profiler) lives in `lp-emu/lp-emu-core`.
- **`lp-riscv-inst`** — RISC-V instruction encoding/decoding.
- **`lp-riscv-elf`** — ELF loading/linking (symbols, relocations, GOT) for
  JIT-compiled guest code.
- **`lp-riscv-emu-guest`** / **`lp-riscv-emu-guest-test-app`** — guest-side
  runtime (syscalls, memory, logging) for code running inside the emulator.
- **`lp-riscv-tools`** — deprecated umbrella crate; use the crates above.

The host↔guest protocol crate formerly here (`lp-riscv-emu-shared`) is now
`lp-emu/lp-emu-abi`; see `docs/adr/2026-07-28-emu-core-crate-family.md`.
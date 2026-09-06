# LightPlayer Xtensa utilities

This directory contains the low-level utilities for the Xtensa architecture
(ESP32-S3 / LX7 and classic ESP32 / LX6 — the two ISAs are identical for the
integer subset LightPlayer uses). It mirrors `lp-riscv/`'s role for rv32:
the instruction model and ELF loading that the shader compiler's `isa/xt`
backend and the Xtensa firmware crates build on, plus the FP conformance
corpus and the rig that runs it on silicon.

The Xtensa **emulator** crates, `lp-xt-emu` and `lp-xt-emu-guest`, now live
in `lp-emu/` with the rest of the emulation family, under its MIT fence.
Crate names did not change. Everything below stayed: it is
compiler-backend and hardware-validation infrastructure, not emulation
(vision Q3). See `lp-emu/README.md` and
`docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md`.

- **`lp-xt-inst`** — Xtensa instruction model, encoder, variable-length
  decoder, and objdump-style disassembler: the integer core plus, since M6,
  the single-precision FP / Boolean-register / SR-UR subset. Everything else
  sits on this crate.
- **`lp-xt-fp-vectors`** — the M6 FP conformance corpus generator: deterministic,
  **float-free**, `no_std`, dependency-free, and fingerprinted, so the host and
  the device generate byte-identical vectors instead of transferring them. Holds
  inputs only; the emulator's predictions live in
  `lp-emu/lp-xt-emu/tests/fixtures/fp/`, committed **before** any hardware run.
- **`lp-xt-fp-harness`** — the rig that runs that corpus on real silicon and
  prints the answers. Shared by `fw-esp32s3` (LX7) and `fw-esp32v3` (LX6), which
  supply only a `BoardId`; it decides nothing, because classification is
  `just fp-diff`'s job against goldens committed before any board ran. DEVICE-target
  crate: in `members` but **not** `default-members` (it depends on `esp-println`,
  so a host `cargo check` cannot build it) — both firmwares compile it for their
  real target in their clippy harness loops.
- **`lp-xt-elf`** — linked-ELF loader + guest-syscall host for
  `lp-emu/lp-xt-emu`, plus a feature-gated relocatable-object engine
  (`R_XTENSA_32` / `R_XTENSA_SLOT0_OP`; the future isa/xt builtins-link path).
- **`lps-builtins-xt-app`** — the Xtensa **builtins image**: a guest
  executable carrying every `__lps_*` builtin at the addresses
  `lp-emu/lp-xt-emu` models, so host-side execution can link compiled shader
  code against real builtins.
  Counterpart of `lp-shader/lps-builtins-emu-app` (rv32). Build it
  with `scripts/build-builtins-xt.sh`; DEVICE-target crate, excluded from the
  host workspace.
- **`lps-builtins-xt-image`** — host crate that **embeds** that image at build
  time and serves it as `&'static [u8]` (empty when unbuilt, which consumers
  treat as "skip the Xtensa host path"). Exists because `lp-shader/*` is sans-IO
  and its consumer `lpvm-native` is also built for firmware; see its README.
- **`fixtures/`** — its own esp-toolchain workspace: the Rust fixture corpus
  (14 guest programs) + hand-written reloc fixtures. Built artifacts are NOT
  checked in — `lp-xt-elf`'s fixture tests skip gracefully until
  `fixtures/build.sh` (and `fixtures/reloc/build.sh`) have been run.

Naming rule (settled in the standalone-core plan): `lp-xt-*` crates are
product code; experiment-only tooling (payload runners, test rigs, the
hardware dual-run harness) stays in the experiment repo.

## Provenance

These crates were built and hardware-verified in the public
[2026-esp32s3-experiment](https://github.com/PhotomancerArt/2026-esp32s3-experiment)
repo and landed here per its `docs/BACKPORT.md`. Instruction encoding data is
derived from `espressif/llvm-project` TableGen sources
(Apache-2.0 WITH LLVM-exception — text vendored at
`licenses/LLVM-Apache-2.0-with-LLVM-exception.txt`); each derived file carries
a provenance header. No GPL source was copied or transliterated; see
`docs/adr/2026-07-29-license-provenance-discipline.md`.

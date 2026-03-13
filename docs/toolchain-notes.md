# Toolchain Notes

The workspace uses **nightly Rust** (`rust-toolchain.toml`). This is required by the
bare-metal firmware targets; host crates compile fine on stable but share the workspace
toolchain for simplicity.

## Why nightly

Three features used by `fw-esp32` and `fw-emu` are unstable:

1. **`#![feature(alloc_error_handler)]`** — Custom OOM handler that panics normally
   (the default handler uses `nounwind` panic, which `catch_unwind` can't intercept).
   Used in `fw-esp32/src/main.rs`.

2. **`-Zbuild-std`** — Rebuilds `core` and `alloc` from source with `panic = "unwind"`.
   The pre-built sysroot for `riscv32imac-unknown-none-elf` uses `panic = "abort"`;
   mixing strategies causes a linker error. Configured in `fw-esp32/.cargo/config.toml`.

3. **`#[lang = eh_personality]`** — Provided by the `unwinding` crate to implement the
   Itanium EH personality routine in `no_std`. This is a lang item, which is unstable.

All three are needed for OOM recovery via stack unwinding on ESP32 and the RISC-V
emulator. See `docs/reports/2026-03-13-esp32-unwinding-implementation.md` for details.

## Alternatives considered

Keeping the workspace on stable with per-crate nightly overrides (`lp-fw/fw-esp32/rust-toolchain.toml`,
etc.) would isolate nightly to firmware builds. This was rejected because:

- Three crates need nightly (`fw-esp32`, `fw-emu`, `emu-guest-test-app`)
- Justfile recipes would need `cd` into each crate directory for the local toolchain
  file to take effect
- The maintenance cost of split toolchains exceeds the risk of nightly regressions on
  host builds

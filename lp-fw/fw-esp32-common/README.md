# fw-esp32-common

Chip-generic firmware layer shared by the per-SOC ESP32 firmware crates —
`fw-esp32c6` today, a future Xtensa `fw-esp32s3` next. Extracted so the second
chip crate reuses the firmware substance instead of forking ~9k LOC.

## Seam rules

Per `docs/adr/2026-07-29-per-chip-fw-toolchains.md`, this crate must build under
BOTH the pinned workspace nightly and the Espressif Xtensa rustc fork:

- No `esp-hal` / `esp-alloc` / `esp-println` / `esp-storage` / `esp-rtos` /
  `esp-radio` dependencies.
- No `unwinding` dependency and no panic-strategy assumptions (the C6 unwinds,
  the S3 aborts — each chip crate owns its posture).
- No `rust-toolchain.toml` and no `.cargo/config.toml` here.

Chip facts arrive by injection instead:

- `server_loop::run_server_loop` takes a `memory_stats` fn (heap free/used) and
  a `feed_watchdog` closure.
- `transport::StreamingMessageRouterTransport::new` takes the chip io-task's
  three embassy channels.
- `hardware::manifest_loader::load_hardware_manifest` takes the compiled-in
  fallback manifest fn.
- `lp_fs::LpFsFlash` is generic over `littlefs_rust::Storage`; the flash
  adapter and partition-derived `Config` stay chip-side.
- `logger` stores injected write fns / a type-erased `dyn SerialIo` handle.

## Contents

`boot`, `server_loop`, `transport`, `time`, `logger`, `jit_fns` (the JIT
host-log symbol), `lp_fs` (littlefs-backed `LpFs`), `hardware::manifest_loader`,
`output::provider` (trait-driven output provider), `output::rmt_state` (lock-free
RMT channel state consumed by chip-side interrupt handlers), `serial::shared_serial`.

The heavy server stack (`lpa-server`, `lpc-model`, `lpc-wire`, `lpfs`,
`lp-recovery`) is behind the `server` feature, mirroring the bin crates.

Deliberately NOT here: RMT buffer fill code (`fw-esp32c6/src/output/rmt/`) —
its pulse codes and buffer geometry are chip constants that must stay
const-folded in the interrupt hot path; app orchestration
(`boot_firmware`/`FirmwareApp`) — deferred until a second chip crate exists to
shape the abstraction.

## Validation

```bash
cargo check -p fw-esp32-common
cargo check -p fw-esp32-common --features server
cargo clippy -p fw-esp32-common --features server -- --no-deps -D warnings
```

The crate is a workspace default-member, so `just check` covers it on the host
toolchain; the firmware builds exercise it under `riscv32imac-unknown-none-elf`.

# fw-esp32-common

Chip-generic firmware layer shared by the per-SOC ESP32 firmware crates —
`fw-esp32c6` today, a future Xtensa `fw-esp32s3` next. Extracted so the second
chip crate reuses the firmware substance instead of forking ~9k LOC.

## Seam rules

Per `docs/adr/2026-07-29-per-chip-fw-toolchains.md`, this crate must build under
BOTH the pinned workspace nightly and the Espressif Xtensa rustc fork:

- No `esp-hal` / `esp-alloc` / `esp-println` / `esp-storage` / `esp-rtos` /
  `esp-radio` dependencies.
- No `unwinding` dependency and no panic-strategy assumptions. Every chip is
  abort tier now (ADR `2026-08-02-rv32-firmwares-are-abort-tier`) — the C6 was
  the last one unwinding — but each chip crate still owns its own posture and
  panic path.
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
`output::provider` (trait-driven output provider), `output::power_gate`
(switched-power-rail state machine behind the manifest's `power_gate`
descriptors — the chip crate supplies the pin and the clock; see
`docs/adr/2026-08-08-switched-power-rail-mechanism.md`), `serial::shared_serial`.

`output::rmt_state` used to sit beside the provider; it went away with its only
consumer when `fw-esp32c6` moved onto `lp-ws281x` (2026-08-01). Per-channel RMT
state is `lp_ws281x::ChannelState` now, shared by all three chips.

The heavy server stack (`lpa-server`, `lpc-model`, `lpc-wire`, `lpfs`,
`lp-recovery`) is behind the `server` feature, mirroring the bin crates.

Deliberately NOT here: RMT register code (`fw-*/src/output/rmt/`) — each chip's
`RmtHw` impl is chip constants that must stay const-folded in the interrupt hot
path, while the refill sequencing above it is `lp-fw/lp-ws281x`; app orchestration
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

# fw-core

`fw-core` contains shared firmware support code used by firmware targets.

It is `no_std` by default and provides reusable pieces for embedded/server
firmware, including serial transport helpers, message routing, test-message
serialization, target-specific logging support, and small runtime loop helpers.

## Relationship To Other Crates

- `fw-esp32` uses `fw-core` with the `esp32` feature for ESP32-C6 firmware.
- `fw-emu` uses `fw-core` with the `emu` feature for RV32 emulator firmware.
- `lpa-server`, `lpc-shared`, `lpc-model`, and `lpc-wire` provide the server,
  shared transport, model, and wire concepts that firmware hosts.

`fw-core` should contain reusable firmware plumbing. Target-specific hardware
setup, board drivers, flash layout, emulator process behavior, and host/browser
runtime lifecycle belong in their target crates.

## Runtime Helpers

`fw_core::runtime` owns target-neutral server loop pieces:

- drain available client messages from a `ServerTransport`
- tick `LpServer` with a computed frame delta
- record last-frame timing on the server

Targets still decide how to boot, yield, sleep, schedule autorun, expose logs,
and manage hardware. This keeps `fw-core` useful without turning it into a
browser, host, or ESP32 abstraction layer.

## Logging

Firmware targets log through the `log` crate. Loggers are constructed
permissive; the process-global `log::max_level()` — seeded to `Info` at init —
is the single runtime gate. Clients can change it at runtime via the wire
`SetLogLevel` command (handled in `lpa-server`); the change is not persisted,
so a reboot reverts to `Info`. See ADR `2026-07-05-studio-logging-model`.

## Features

- `std`: enables host-side support for tests and logging dependencies.
- `emu`: enables emulator-specific logging/serialization support.
- `esp32`: enables ESP32-specific firmware support.

## Validation

```bash
cargo check -p fw-core
cargo test -p fw-core
```

When changing code that affects firmware behavior, also run the relevant target
checks from the root `AGENTS.md`, especially `fw-esp32` and `fw-emu` target
checks.

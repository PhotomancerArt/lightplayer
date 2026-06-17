# fw-esp32

`fw-esp32` is the reference embedded LightPlayer firmware target for ESP32-C6.

This is the main bare-metal product path: GLSL shaders are compiled on the
device at runtime and executed from RAM. Do not replace this with host/browser
precompilation, and do not feature-gate the compiler out of the embedded
compile/execute path to solve build, size, or `no_std` issues.

## Responsibilities

- ESP32-C6 boot and board initialization.
- USB/JTAG serial transport.
- Flash-backed or memory-backed LightPlayer filesystem.
- `lp-server` hosting on device.
- LED output through RMT/WS281x drivers.
- Root-owned hardware capabilities such as buttons and ESP-NOW radio support.
- Firmware check and test harness modes behind feature flags.

Shared firmware plumbing belongs in `fw-core`. Host-local runtime lifecycle
belongs in `fw-host`. Browser Studio simulation belongs in `fw-browser`.

## Common Commands

Run on a connected ESP32-C6:

```bash
just demo-esp32
```

Target check from the workspace root:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

For linked firmware builds, size measurements, or bloat analysis, run from this
crate directory so the crate-local linker configuration is active:

```bash
cd lp-fw/fw-esp32
cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
rust-size ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32
```

## Feature Notes

The default feature set targets ESP32-C6 with server and radio support. Many
`test_*` features select focused firmware harnesses for hardware validation,
profiling, or smoke tests. Keep feature additions honest: test and check modes
may narrow behavior for a harness, but the normal firmware path must preserve
runtime shader compilation on device.

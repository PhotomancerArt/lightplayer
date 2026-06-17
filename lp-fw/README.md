# LightPlayer Firmware And Local Runtimes

This directory contains LightPlayer firmware and firmware-shaped runtime targets.
The core product path is still embedded GLSL JIT execution: shaders are compiled
and run on the target device at runtime. Host and browser runtimes exist to make
local development, Studio simulation, and non-embedded deployments practical;
they are not replacements for on-device shader compilation.

## Crates

| Crate | Target | Purpose |
|---|---|---|
| [`fw-esp32`](./fw-esp32/) | ESP32-C6 bare metal | Reference embedded firmware target. Runs `lp-server` on device. |
| [`fw-emu`](./fw-emu/) | RV32 bare-metal emulator | Firmware image used by emulator-oriented validation. |
| [`fw-host`](./fw-host/) | Host OS | Local host runtime that can run an in-memory `LpServer` outside `lp-cli`. Useful for Studio, local services, and host deployments. |
| [`fw-browser`](./fw-browser/) | `wasm32-unknown-unknown` browser/Web Worker | Browser runtime proof for Studio project simulation and browser-local testing. |
| [`fw-core`](./fw-core/) | shared | Shared firmware support code. |
| [`fw-tests`](./fw-tests/) | host test harness | Firmware/emulator integration tests. |
| [`fw-checks`](./fw-checks/) | host checks | Firmware validation/check helper crate. |

## Target Roles

### Embedded Firmware

`fw-esp32` and `fw-emu` preserve the embedded product path. They must keep the
GLSL compiler and runtime execution available on the target. Do not feature-gate
the compiler out of these targets to work around build, size, or `no_std`
issues.

### Host Runtime

`fw-host` is the host-OS LightPlayer runtime target. It owns reusable local
server lifecycle that should not live only in `lp-cli`. The Studio link layer can
use this target through `lpa-link` `local-host` support to create local runtime
instances and connect an `lpa-client` to them.

Useful checks:

```bash
cargo check -p fw-host
cargo test -p fw-host
cargo check -p lpa-link --features local-host
cargo test -p lpa-link --features local-host
```

### Browser Runtime

`fw-browser` is the browser/Web Worker runtime target for Studio simulation and
project testing. It builds to wasm, initializes the browser `lpvm-wasm` runtime,
owns an in-memory `LpServer`/filesystem/virtual hardware runtime, accepts
`lpc_wire` client frames over a structured worker envelope, and can load/tick a
project without exposing direct shader APIs to JavaScript.

Useful checks:

```bash
cargo check -p fw-browser --target wasm32-unknown-unknown
cargo test -p fw-browser --target wasm32-unknown-unknown --no-run
just fw-browser-build
```

To manually run the browser smoke page:

```bash
just fw-browser-smoke
```

Then open:

```text
http://127.0.0.1:2819/smoke.html
```

Success means the page shows `ok` and
`document.documentElement.dataset.smoke == "ok"`. The current page writes a
small project through worker messages, loads it, ticks the runtime, and verifies
increasing output bytes through project-read `OutputChannels` resources.

`just fw-browser-test` is the intended automated `wasm-bindgen-test` path, but it
requires a working browser/WebDriver environment. If it fails locally because no
headless browser is available, treat that as browser-runner provisioning rather
than proof that `fw-browser` failed to compile.

## Running On Device

### ESP32-C6

To run the firmware on an ESP32-C6 device:

```bash
just demo-esp32
```

This will:

1. Ensure the RISC-V 32-bit target is installed.
2. Build and flash the firmware to the connected ESP32-C6 device.
3. Run the firmware on the device.

The command is equivalent to:

```bash
cd lp-fw/fw-esp32
cargo run --target riscv32imac-unknown-none-elf --release --features esp32c6
```

Requirements:

- ESP32-C6 device connected via USB.
- `cargo-espflash` or `espflash` installed.
- RISC-V 32-bit target installed, usually handled by the just recipe.

For linked ESP32 builds, size measurements, and bloat analysis, run from
`lp-fw/fw-esp32/` or through a just recipe that changes into that directory so
the crate-local linker configuration is active.

## Workspace Notes

This workspace mixes host crates, browser wasm crates, and RV32 bare-metal
firmware crates. Do not use `cargo build --workspace` or
`cargo test --workspace` on the host target. Prefer targeted checks or the
repo-level just recipes documented in the root `AGENTS.md`.

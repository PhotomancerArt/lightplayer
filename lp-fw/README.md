# LightPlayer Firmware And Local Runtimes

This directory contains LightPlayer firmware and firmware-shaped runtime targets.
The core product path is still embedded GLSL JIT execution: shaders are compiled
and run on the target device at runtime. Host and browser runtimes exist to make
local development, Studio simulation, and non-embedded deployments practical;
they are not replacements for on-device shader compilation.

## Crates

| Crate | Target | Purpose |
|---|---|---|
| [`fw-esp32c6`](./fw-esp32c6/) | ESP32-C6 bare metal (RISC-V) | Reference embedded firmware target. Runs `lp-server` on device, with every node kind and every driver. |
| [`fw-esp32s3`](./fw-esp32s3/) | ESP32-S3 bare metal (Xtensa LX7) | Second chip. Runs `lp-server` on device, JITs GLSL to **Xtensa** machine code, and drives real WS281x strips on 4 concurrent RMT channels via `lp-ws281x`. Deliberately partial: shader + fixture nodes only. See its README for what is gated off and why. |
| [`fw-esp32-common`](./fw-esp32-common/) | chip-generic lib | Chip-generic firmware layer shared by the per-SOC ESP32 crates — both `fw-esp32c6` and `fw-esp32s3` consume it. Builds under both the pinned nightly and the Espressif fork; no esp-* HAL deps. |
| [`lp-ws281x`](./lp-ws281x/) | chip-agnostic `no_std` lib | Portable core of the multi-channel WS2811/WS2812 RMT driver — pulse encoding, ping-pong refill, guard-word flicker protection — behind the `RmtHw` trait a chip backend implements. Used by `fw-esp32s3` today; the C6 stays on its own legacy single-channel driver (see `docs/debt/`). |
| [`fw-emu`](./fw-emu/) | RV32 bare-metal emulator | Firmware image used by emulator-oriented validation. |
| [`fw-host`](./fw-host/) | Host OS | Local host runtime that can run an in-memory `LpServer` outside `lp-cli`. Useful for Studio, local services, and host deployments. |
| [`fw-browser`](./fw-browser/) | `wasm32-unknown-unknown` browser/Web Worker | Browser runtime proof for Studio project simulation and browser-local testing. |
| [`fw-core`](./fw-core/) | shared | Shared firmware support code. |
| [`fw-tests`](./fw-tests/) | host test harness | Firmware/emulator integration tests. |
| [`fw-checks`](./fw-checks/) | host checks | Firmware validation/check helper crate. |

## Firmware Manifest Core

Every firmware artifact embeds a **manifest core** — a small JSON blob
describing the build: package, target, enabled `LpFeature`s, flash limits,
wire proto, and provenance. It is assembled at compile time from the same
`cfg!` facts as the feature gates themselves (via
`lpc_model::lp_embed_manifest_core!` in each embedder's root) and extracted
by scanning the artifact — never re-stated by tooling:

```bash
lp-cli firmware show target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
node scripts/extract-fw-manifest.mjs <artifact> --stable   # CI twin, no build needed
```

Each firmware crate checks in `manifest-core.expected.json`
(provenance-stripped); CI extracts the manifest from the image it just built
and diffs against it (`just fw-manifest-check-esp32c6` / `-esp32s3` /
`-emu`), so a PR that changes a build's feature set changes the fixture
visibly. A new embedder invokes the macro with its own facts; engine feature
truth arrives via `lpa_server::ENGINE_FEATURE_FRAGMENT` (or
`lpc_engine::features::` for direct dependents). See
`docs/adr/2026-08-01-firmware-manifest-architecture.md`.

## Target Roles

### Embedded Firmware

`fw-esp32c6`, `fw-esp32s3`, and `fw-emu` preserve the embedded product path.
They must keep the GLSL compiler and runtime execution available on the target.
Do not feature-gate the compiler out of these targets to work around build,
size, or `no_std` issues.

The per-node-kind gates on `fw-esp32s3` are **not** an exception to that rule
and must not be read as precedent for one: they remove *node runtimes*, never
the compiler. That build compiles and executes GLSL on the board — it is the
only reason it exists.

### Host Runtime

`fw-host` is the host-OS LightPlayer runtime target. It owns reusable local
server lifecycle that should not live only in `lp-cli`. The Studio link layer can
use this target through `lpa-link` `host-process` support to create local runtime
instances and connect an `lpa-client` to them.

Useful checks:

```bash
cargo check -p fw-host
cargo test -p fw-host
cargo check -p lpa-link --features host-process
cargo test -p lpa-link --features host-process
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

For a pass/fail verdict on the browser smoke page:

```bash
just fw-browser-smoke-check
```

It runs `smoke.html` in headless Chrome and exits non-zero unless the page
reaches `dataset.smoke == "ok"`.

To watch the page instead:

```bash
just fw-browser-smoke
```

Then open the URL the recipe prints (its port comes from
`scripts/dev-port.sh`). That recipe only serves the page — it never exits and
cannot fail on its own.

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
cd lp-fw/fw-esp32c6
cargo run --target riscv32imac-unknown-none-elf --release --features esp32c6
```

Requirements:

- ESP32-C6 device connected via USB.
- `cargo-espflash` or `espflash` installed.
- RISC-V 32-bit target installed, usually handled by the just recipe.

For linked ESP32 builds, size measurements, and bloat analysis, run from
`lp-fw/fw-esp32c6/` or through a just recipe that changes into that directory so
the crate-local linker configuration is active.

### Studio Firmware Package

Studio browser flashing consumes prebuilt ESP32-C6 firmware assets rather than
building from an ELF in the browser. Generate the current browser-flashable
package with:

```bash
just studio-firmware-package-esp32c6
```

The recipe builds `fw-esp32c6` with `esp32c6,server` under the `release-esp32`
profile, then runs `espflash save-image --merge --skip-padding` to emit a merged
binary image and manifest under:

```text
lp-app/lpa-studio-web/public/firmware/esp32c6/
```

The package is generated output and is gitignored. `just studio-web-build`
depends on this package so release/static Studio builds have the firmware assets
available for the browser provisioning flow.

## Workspace Notes

This workspace mixes host crates, browser wasm crates, and RV32 bare-metal
firmware crates. Do not use `cargo build --workspace` or
`cargo test --workspace` on the host target. Prefer targeted checks or the
repo-level just recipes documented in the root `AGENTS.md`.

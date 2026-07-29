# fw-esp32c6

`fw-esp32c6` is the reference embedded LightPlayer firmware target for ESP32-C6.

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

Chip-generic firmware logic (server loop, transport, logger, boot, output
provider, littlefs glue) lives in `fw-esp32-common` and is consumed here;
this crate keeps only what is genuinely ESP32-C6: board init, RMT register
code, USB-Serial-JTAG, recovery backend + `panic=unwind`/`__eh_frame`
machinery, and the hardware test harnesses (see
`docs/adr/2026-07-29-per-chip-fw-toolchains.md` for the seam rules).
Shared firmware plumbing belongs in `fw-core`. Host-local runtime lifecycle
belongs in `fw-host`. Browser Studio simulation belongs in `fw-browser`.

## Common Commands

Run on a connected ESP32-C6:

```bash
just demo-esp32
```

Target check from the workspace root:

```bash
cargo check -p fw-esp32c6 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

For linked firmware builds, size measurements, or bloat analysis, run from this
crate directory so the crate-local linker configuration is active:

```bash
cd lp-fw/fw-esp32c6
cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
rust-size ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
```

## Flash Budget And Diagnostics

The app image must fit a 3 MB partition. `.cargo/config.toml` buys ~155 KB of
that by giving up on-device diagnostics, and `build-std`'s `optimize_for_size`
adds ~50 KB more:

| Flag | Saves | Cost |
|---|---|---|
| `-Zlocation-detail=none` | 59,488 B | panics lose `file:line` |
| `-Zfmt-debug=none` | 95,584 B | `{:?}` formats to nothing |
| `optimize_for_size` (build-std) | 51,344 B | none measured (render loop unaffected) |

**When you need real panic output while debugging, delete the `fmt-debug` line
first** — it is the one that turns `panicked at src/foo.rs:12: bad state {x:?}`
into `panicked at <redacted>:0:0:` with an empty payload. Drop
`location-detail` too if you need line numbers; both are one-line reverts, and
neither is needed for a local debug build to be correct.

Note that `ESP_LOG` does *not* control this firmware's own log level: the
logger installs a runtime `log::max_level()` (see `src/logger.rs`) seeded to
Info and changeable from the client with the wire `SetLogLevel` command.

Check headroom at any time — this is the same check pre-merge CI runs:

```bash
just fw-esp32c6-size-check
```

Background and the decisions behind the budget (including why the ~500 KB WiFi
blob is kept and what the lpfs partition is reserved for) are in
`docs/adr/2026-07-28-esp32c6-flash-budget.md`.

## Feature Notes

The default feature set targets ESP32-C6 with server and radio support. Many
`test_*` features select focused firmware harnesses for hardware validation,
profiling, or smoke tests. Keep feature additions honest: test and check modes
may narrow behavior for a harness, but the normal firmware path must preserve
runtime shader compilation on device.

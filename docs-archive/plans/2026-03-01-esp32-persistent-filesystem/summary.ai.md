# Summary: ESP32 Persistent Filesystem

## Implemented

1. **lp-cli upload command** – `lp-cli upload <dir> <host>` connects, stops all projects, pushes, loads, and exits. Non-interactive.
2. **Partitions** – `partitions.csv` with app 3MB, lpfs 1MB. `just demo-esp32c6-host` uses `--partition-table` and `upload` instead of `dev`.
3. **flash_fs feature** – Optional esp-storage and littlefs2 deps; not default (see blocker below).
4. **demo_project removed** – No embedded demo; use `lp-cli upload` to push projects.
5. **LightplayerConfig** – `lp-model::config::LightplayerConfig { startup_project: Option<String> }` for future boot config.

## Blocked: littlefs2-sys

littlefs2-sys fails to build for `riscv32imac-unknown-none-elf` (bindgen/clang cannot find `uint32_t` and other stdint types when targeting bare-metal). Flash FS implementation (Storage adapter, LpFsFlash, boot auto-load) depends on littlefs2 and is blocked until this is resolved.

Possible workarounds:
- Set `BINDGEN_EXTRA_CLANG_ARGS` to include host stdint.h (e.g. on macOS: `-include /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/stdint.h`)
- Use ESP toolchain's compiler for C/bindgen
- Investigate littlefs2-sys or trussed-dev/littlefs2 for bare-metal build fixes

## Current Flow

1. `just demo-esp32c6-host` – Flash firmware (with partition table) → `lp-cli upload examples/basic serial:auto`
2. Firmware uses LpFsMemory (no persistence yet). After flash_fs is unblocked, firmware will use littlefs and projects will persist across reboots.

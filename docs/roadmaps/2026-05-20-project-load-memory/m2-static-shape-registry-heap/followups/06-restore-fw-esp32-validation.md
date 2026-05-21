# Restore FW ESP32 Validation

## Status

Implemented. A fresh ESP32 server check passed after the external checkout was
repaired:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Smell

The final cleanup pass could not get a fresh `fw-esp32` check because the build
is blocked in the external `esp-storage` checkout:

```text
/Users/yona/dev/photomancer/feature/oss/esp-hal/esp-storage/src/nor_flash.rs
unexpected closing delimiter
```

`fw-emu` passed, but the ESP32 target should be checked again after that
external syntax issue is fixed.

## Better Shape

Keep this file as the record that the deferred ESP32 validation was restored.

## Useful Context

- `fw-emu` passed on this branch after cleanup.
- The failure occurs before the static-shape changes are meaningfully checked
  for `fw-esp32`.

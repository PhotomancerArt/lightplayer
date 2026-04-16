# Phase 3: flash_fs feature and dependencies

## Scope of phase

Add flash_fs feature with optional esp-storage and littlefs2. Keep it non-default (littlefs2-sys has build issues on bare-metal targets).

## Implementation Details

- Add flash_fs feature, esp-storage and littlefs2 as optional deps
- flash_fs not in default (user opts in)

## Validate

```bash
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --no-default-features --features "esp32c6,server"
```

# LightPlayer Firmware

This directory contains the firmware for LightPlayer, the bare-metal, no_std, `lp-server`
implementations that run on various microcontrollers.

## Running on Device

### ESP32-C6

To run the firmware on an ESP32-C6 device:

```bash
just demo-esp32
```

This will:
1. Ensure the RISC-V 32-bit target is installed
2. Build and flash the firmware to the connected ESP32-C6 device
3. Run the firmware on the device

The command is equivalent to:
```bash
cd lp-fw/fw-esp32 && cargo run --target riscv32imac-unknown-none-elf --release --features esp32c6
```

**Requirements:**
- ESP32-C6 device connected via USB
- `cargo-espflash` or `espflash` installed (usually installed automatically by cargo-espflash)
- RISC-V 32-bit target installed (handled automatically by the just command) 
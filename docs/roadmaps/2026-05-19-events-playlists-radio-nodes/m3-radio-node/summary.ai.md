# M3 Control Radio Summary

Implemented `ControlRadio` as the symmetric radio bridge for the first fyeah sign sync slice.

## Done

- Added `ControlRadioDef` and `ControlRadioState`.
- Added `NodeKind::ControlRadio` and `NodeDef::ControlRadio`.
- Added `RadioService` plumbing through engine, server, emulator firmware, and ESP32 firmware.
- Added `ControlRadioNode`.
- Supported one bidirectional graph binding:

```toml
kind = "ControlRadio"

[bindings.input]
source = "bus#trigger"

[bindings.output]
target = "bus#trigger"
```

- Used `RadioMessageKind::ControlMessage` with an 8-byte payload: little-endian `id`, then little-endian `seq`.
- Published an empty `output` map before resolving `input`, then published accepted messages after local/remote processing. This lets `input` and `output` both bind to `bus#trigger` without re-entering the radio node through its own output.
- Added bounded repeated sends for local messages and receiver-side dedupe on `(id, seq)`.
- Added `examples/button-sign`, expanding the button playlist example with the symmetric
  `ControlRadio` binding on `bus#trigger`.
- Deferred ack, ownership, TTL, retransmit windows, and mesh behavior to future radio-sync work.

## Validation

- `cargo check -p lpc-model`
- `cargo check -p lpc-engine`
- `cargo check -p lpa-server`
- `cargo test -p lpc-model control_radio`
- `cargo test -p lpc-engine button_sign_example_loads_with_control_radio_node`
- `cargo test -p lpc-engine control_radio_bidirectional_bus_binding_broadcasts_button_event`
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

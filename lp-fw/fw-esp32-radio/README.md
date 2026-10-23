# fw-esp32-radio

The ESP32 radio glue the chip firmware crates share: everything between
esp-radio and LightPlayer's own radio code. Its consumers are `fw-esp32c6`
and, since the S3 radio plan's M3 (`lp2025/2026-10-20-2335-s3-radio-and-ota`),
`fw-esp32s3`'s `radio`, `ble` and `wifi` (M3, M4; BTDM controller config in
`fw-esp32s3/src/ble/ble_controller_config.rs`, its `NetChip` in
`fw-esp32s3/src/net/net_chip.rs`).

It moved here from `fw-esp32c6/src/` as-is (M2 of that plan). The C6's
behaviour, log lines, figures and boot order did not change.

## What lives here

| Module | Feature | What it is |
|---|---|---|
| `hardware::radio_hub` | `radio` | The radio's one bring-up (`esp_radio::wifi::new`) and who gets its parts |
| `hardware::espnow_radio_driver` | `radio` | The ESP-NOW radio driver behind the hardware registry, and the Radio node rule while Wi‑Fi is in use |
| `hardware::espnow_controller_config` | `radio` | The lean Wi‑Fi driver counts every bring-up that mirrors the product's takes |
| `ble` | `ble` | The BLE link: bring-up, the host runner, advertising, one lp-link per connection over the Nordic-UART GATT service, the HCI transport that closes connections on a controller reset |
| `net` | `wifi` | The network on `lp-net`: the thread, the station task over esp-radio's station, embassy-net, the LAN endpoint, mDNS, the relay's device leg, and the boards the main thread reads them through |

The chip-free half below all of this — the join policy, the link mux, the
WebSocket and mDNS codecs, the relay client, the network seam's adapters —
is `fw-esp32-common`, which by rule never depends on esp-hal or esp-radio.
This crate is the esp-radio-aware layer between it and a chip crate.

## The rule: chip facts come in

Nothing here names a chip. What esp-hal answers the same way on every chip
(the efuse base MAC, the hardware RNG, `RawPriorityLimitedMutex`) is called
directly. What a chip decides, the chip crate passes in:

| Fact | How it arrives | The C6's |
|---|---|---|
| The BLE controller's configuration | `ble::start(.., config)` | `fw-esp32c6/src/ble/ble_controller_config.rs` (NPL setters; the S3's controller is BTDM) |
| Board quirks before BLE (the XIAO-C6's RF switch) | the chip runs them before it calls `ble::start` | `fw-esp32c6/src/ble/mod.rs` takes the `BoardQuirksApplied` proof |
| `lp-net`'s stack, priority and core | `net::net_thread::NetChip` | `fw-esp32c6/src/net/net_chip.rs`: 8 KB, priority 1, unpinned (one core); the S3's: 12 KB, priority 1, core 0 |
| First light (the boot's first join waits for it) | `NetChip::first_light_decided` | `crate::first_light::decided` |
| Diagnostics (stack paint, heap tracking, radio DMA) | `NetChip::on_thread_start`, `NetChip::on_join_attempt`, `net_heartbeat::log_line(on_joined)` | no-ops unless the C6's diag features are on |
| The network seam's wake (RISC-V only) | `NetChip::bind_seam_wake` | `crate::seams::seam_wake_handler::bind` |
| The relay's address and the build's version | `net_thread::start(.., relay)` | `fw-esp32c6/src/net/relay_config.rs` (`LP_RELAY_HOST` is a build fact) |
| The radio's C heap region | not here at all | `fw-esp32c6/src/c_heap.rs` (`HEAP_RADIO`) |

The hooks are plain `fn` pointers, not generics: an image has one chip, and
a generic over a chip would be one more thing to monomorphise (AGENTS.md,
"How to Handle Binary Size Issues", item 6).

The network seam (`net=lan`) is RISC-V only, like its calls in
`fw-esp32-common`, so the seam arms of `net::board_station::BoardStation` and
`net::esp_frame_device::BoardFrameDevice` compile only on `riscv32`;
elsewhere the radio always runs.

## Features

- `radio`, `ble`, `wifi`: the same tiers as the C6's features of those names,
  which turn these on. Each one implies the one before.
- `esp32c6`, `esp32s3`: the chip, forwarded to esp-hal, esp-rtos, esp-radio
  and esp-println, and nothing else.
- `desk_ble_params`, `desk_ble_fault`: desk-only BLE builds, never shipped;
  the C6's features of the same names turn them on.

Every dependency's features are a subset of what `fw-esp32c6` already turns
on, so the C6's resolved feature set did not change with the move.

## Building and linting

There is no host build: esp-hal and esp-radio build only for a chip target,
so this crate is a workspace member but not a default member, like the chip
crates. It has no `rust-toolchain.toml` and no `.cargo/config.toml`; it
builds inside a chip crate's build.

```bash
just clippy-fw-esp32c6        # includes this crate, at each feature tier, on the C6's target
just fw-esp32c6-size-check    # the C6's image, which links it
```

Log lines keep the text they had in the C6 (`[fw-esp32c6] ESP-NOW …`, `[ble]`,
`[wifi]`, `[relay]`): walks and desk procedures read them.

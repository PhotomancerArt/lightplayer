//! The Wi-Fi driver configuration the product brings ESP-NOW up with.
//!
//! The product runs `esp_radio::wifi::new` for ESP-NOW alone: it never joins
//! a network, and ESP-NOW broadcasts are short, unaggregated frames at a few
//! hertz. esp-radio's `ControllerConfig::default()` is sized for a station
//! moving bulk traffic — 10 static RX buffers, 32 dynamic RX, 32 dynamic TX,
//! a 6-frame block-ack window — and all of it lands in the radio's C heap,
//! which lived in the reclaimed `dram2_seg` region until 2026-10-05 and has
//! its own region in main RAM since (`docs/adr/2026-09-02-esp32c6-ram-split.md`,
//! `src/c_heap.rs`).
//!
//! The lean counts below are the ones measured on silicon on 2026-10-01
//! (XIAO ESP32-C6, PLAYFUL Choker; planning
//! `lp2025/2026-10-01-0300-wifi-control-experiments`): the radio's
//! `dram2_seg` use at `wifi::new` falls from 30,404 B to 20,084 B, and a
//! joined station showed no throughput or round-trip change (the render loop,
//! not the radio, bounds both). esp-radio's own rule holds: `rx_ba_win` is
//! below both `dynamic_rx_buf_num` and twice `static_rx_buf_num`.
//!
//! What it costs: the RX DMA ring is 4 descriptors, not 10, so a burst of
//! more than four frames arriving before the Wi-Fi task drains the ring
//! loses the rest. ESP-NOW at the product's rates never gets near that.
//!
//! Every bring-up that claims to mirror the product's (the stress builds,
//! the desk ESP-NOW meter, the BLE coexistence harness) takes this function
//! rather than its own `ControllerConfig`, so what they measure is the
//! radio a board actually runs.

use esp_radio::wifi::ControllerConfig;

/// Static RX buffers: the RX DMA ring the MAC writes into.
const STATIC_RX_BUF_NUM: u8 = 4;
/// Dynamic RX buffers: frames handed up from the ring, awaiting the stack.
const DYNAMIC_RX_BUF_NUM: u16 = 8;
/// Dynamic TX buffers: frames queued for the air.
const DYNAMIC_TX_BUF_NUM: u16 = 8;
/// Block-ack RX window (A-MPDU reordering; a joined station only).
const RX_BA_WIN: u8 = 3;

/// The `ControllerConfig` the product's ESP-NOW radio is brought up with.
pub fn espnow_controller_config() -> ControllerConfig {
    ControllerConfig::default()
        .with_static_rx_buf_num(STATIC_RX_BUF_NUM)
        .with_dynamic_rx_buf_num(DYNAMIC_RX_BUF_NUM)
        .with_dynamic_tx_buf_num(DYNAMIC_TX_BUF_NUM)
        .with_rx_ba_win(RX_BA_WIN)
}

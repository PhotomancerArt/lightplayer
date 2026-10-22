//! The ESP32 radio glue the chip firmware crates share (README.md).
//!
//! Everything here calls esp-radio's chip-independent API — `WifiController`,
//! `Interface`, `EspNow`, `BleConnector` — and esp-hal's (the efuse MAC, the
//! RNG, `RawPriorityLimitedMutex`), which esp-hal answers the same way on
//! every chip. What *is* a chip fact comes in from the chip crate: the BLE
//! controller's configuration (`ble::start`), the network thread's stack,
//! priority and hooks (`net::net_thread::NetChip`), and the board's quirks
//! (the chip crate runs them before it starts BLE).
//!
//! - [`hardware`]: the radio's one bring-up and the ESP-NOW driver (`radio`);
//! - [`ble`]: the BLE link (`ble`);
//! - [`net`]: the network on `lp-net` (`wifi`).

#![no_std]

extern crate alloc;

#[cfg(feature = "ble")]
pub mod ble;
#[cfg(feature = "radio")]
pub mod hardware;
#[cfg(feature = "wifi")]
pub mod net;

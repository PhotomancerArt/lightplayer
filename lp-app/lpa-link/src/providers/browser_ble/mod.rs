//! Web Bluetooth: the same `M!{json}` line protocol, over a Nordic UART
//! (NUS) GATT service (M5 of the BLE remote-control plan).
//!
//! BLE is *just another transport*. It follows the Web Serial adapter's
//! shape: the JS module owns the device and its connection, a thin Rust
//! binding reaches it through `#[wasm_bindgen(module = …)]` with no
//! `web-sys` Bluetooth features, commands run in spawned futures, and
//! events queue for `poll_event`. Bytes become lines through the SAME
//! `LineSplitter` every byte transport uses.
//!
//! | file | what it owns |
//! |---|---|
//! | `browser_ble.js` | the `BluetoothDevice`, the bounded connect, the awaited chunked writes, the reconnect loop, visibility re-checks, presence edges |
//! | `browser_ble.rs` | the bindings and the session descriptor ([`BleDevice`]) |
//! | `ble_wire.rs` | [`BleWire`] — a session's byte stream and its one `WireStream`, shared by the link and a borrowing conversation |
//! | `ble_client_io.rs` | [`BleClientIo`] — `lpa-client`'s io over the wire, for push/remove/manifest writes and the editor lens |
//!
//! The model's `Link` over this lives in `device_link::browser_ble`.
//!
//! What a Bluetooth link cannot do is stated once, in
//! [`LinkProviderKind::BrowserBle`](crate::LinkProviderKind::BrowserBle)'s
//! capabilities: no reset, no flash, no erase, no boot control, no raw
//! filesystem. There are no DTR/RTS lines on the far side of a GATT service.
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The browser half is pinned
//! by `tests/browser_ble_conformance.rs` (`just lpa-link-browser-test`),
//! against the `?ble=emu` polyfill.

mod ble_client_io;
mod ble_wire;
mod browser_ble;

pub use ble_client_io::{BleClientIo, BleTapLine};
pub use ble_wire::{BleWire, is_link_lost};
pub use browser_ble::{
    BleAvailability, BleBrowser, BleDevice, availability, forget, install_ble_events, is_supported,
    present_devices, recheck_all, request_device, restore_granted_devices,
};

//! The radio's two peripherals, kept from `init_board` (the one place the
//! peripheral singleton is split) until `core_boot` brings the radio up
//! after the filesystem mounts: `WIFI` for the radio's one bring-up
//! (`fw_esp32_radio::hardware::radio_hub`), `BT` for the BLE link.

use core::cell::RefCell;

use critical_section::Mutex;
use esp_hal::peripherals::{BT, WIFI};

static WIFI_PERIPHERAL: Mutex<RefCell<Option<WIFI<'static>>>> = Mutex::new(RefCell::new(None));
static BT_PERIPHERAL: Mutex<RefCell<Option<BT<'static>>>> = Mutex::new(RefCell::new(None));

/// Keep both for [`take_wifi`] and [`take_bt`].
pub fn stash(wifi: WIFI<'static>, bt: BT<'static>) {
    critical_section::with(|cs| {
        WIFI_PERIPHERAL.borrow_ref_mut(cs).replace(wifi);
        BT_PERIPHERAL.borrow_ref_mut(cs).replace(bt);
    });
}

/// The WIFI peripheral, once.
pub fn take_wifi() -> Option<WIFI<'static>> {
    critical_section::with(|cs| WIFI_PERIPHERAL.borrow_ref_mut(cs).take())
}

/// The BT peripheral, once.
#[cfg(s3_ble)]
pub fn take_bt() -> Option<BT<'static>> {
    critical_section::with(|cs| BT_PERIPHERAL.borrow_ref_mut(cs).take())
}

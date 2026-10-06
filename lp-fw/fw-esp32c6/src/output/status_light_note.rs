//! The engine's half of the update light (OTA plan DM22): when a project's
//! first WS281x output opens, the engine records which strip it is in
//! `/.lp/status-light.json` (`lpc_update::StatusLightRecord`, format 1),
//! so that the split image's core — which has no engine and never parses the
//! hardware manifest — can light it while an update runs.
//!
//! The RMT driver notes the pad and the LED count of the first output it
//! opens this boot ([`note_open`]); the server loop's frame hook
//! ([`persist`], installed by a split image) writes the record through the
//! server's filesystem, **only when it differs** from what is on disk (lpfs
//! wear-levels; a write per boot would still be a write per boot).
//!
//! The record's `colorOrder` is `"grb"`: the WS2812-class byte order every
//! channel of the C6's driver is opened with. The fixture node's own colour
//! order lives above the output provider, per lamp, where this driver never
//! sees it; a strip wired in another order shows the light in swapped
//! colours (dark yellow stays yellow under the common R/G swap; dark red
//! does not).

use core::cell::Cell;

use critical_section::Mutex;
use lpa_server::LpServer;
use lpc_update::{STATUS_LIGHT_PATH, StatusLightRecord};
use lpfs::lp_path::AsLpPath;

/// The first output this boot opened: `(gpio, leds)`, and whether the record
/// is settled.
static FIRST: Mutex<Cell<(Option<(u8, u32)>, bool)>> = Mutex::new(Cell::new((None, false)));

/// The byte order the C6's driver opens every channel with.
const WIRE_ORDER: &str = "grb";

/// The RMT driver opened an output on `gpio` driving `leds` LEDs.
pub fn note_open(gpio: u8, leds: u32) {
    critical_section::with(|cs| {
        let cell = FIRST.borrow(cs);
        let (first, settled) = cell.get();
        if first.is_none() {
            cell.set((Some((gpio, leds)), settled));
        }
    });
}

/// The frame hook: write the record once, if it changed.
pub fn persist(server: &LpServer) {
    let (first, settled) = critical_section::with(|cs| FIRST.borrow(cs).get());
    let Some((gpio, leds)) = first.filter(|_| !settled) else {
        return;
    };
    critical_section::with(|cs| FIRST.borrow(cs).set((first, true)));
    let record = StatusLightRecord::ws281x(gpio, leds, WIRE_ORDER).to_json();
    let fs = server.base_fs();
    let path = STATUS_LIGHT_PATH.as_path();
    if fs.read_file(path).ok().as_deref() == Some(record.as_slice()) {
        return;
    }
    match fs.write_file(path, &record) {
        Ok(()) => log::info!("[OTA] status light: GPIO{gpio} × {leds} LEDs recorded"),
        Err(e) => log::warn!("[OTA] status light: could not record it: {e}"),
    }
}

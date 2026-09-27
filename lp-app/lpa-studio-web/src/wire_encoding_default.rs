//! Whether this page asks boards to pack their replies — and the one place
//! it does not: Web Serial on macOS.
//!
//! Chromium opens a macOS tty with `PARMRK` set and `IGNBRK` clear, so the
//! kernel stores every `0xFF` byte twice and the tty silently drops about a
//! kilobyte when its queue wraps. Packed replies (JSON Pack) carry `0xFF`;
//! JSON lines never do. Until the board and Studio speak `lp-link` (which
//! keeps `0x00` and `0xFF` off the wire and resends what is lost), a page on
//! a Mac asks for JSON on real Web Serial. Defect
//! `docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md`,
//! ADR `docs/adr/2026-09-27-lp-link-one-comms-layer.md`.
//!
//! What it leaves alone: `?emu=` pages (their `navigator.serial` is the
//! emulator's shim, which has no tty), Bluetooth (which never asks for
//! packed), every other OS, and `lp-cli` (native termios, no `PARMRK`).
//! `?wire=packed` / `?wire=json` override it for a measurement.

use crate::dev_url_flags::WireChoice;

/// Whether a page asks boards for packed replies.
///
/// `wire` is the `?wire=` override, `emu` whether `?emu=` replaced
/// `navigator.serial`, `macos` whether the browser runs on macOS.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm install; host builds only run the unit tests"
    )
)]
pub fn packed_replies_wanted(wire: Option<WireChoice>, emu: bool, macos: bool) -> bool {
    match wire {
        Some(WireChoice::Json) => false,
        Some(WireChoice::Packed) => true,
        None => emu || !macos,
    }
}

/// Decide for this page and tell the browser readers, before any device
/// connects. Says so in the console whenever the page does not pack.
#[cfg(target_arch = "wasm32")]
pub fn install(wire: Option<WireChoice>, emu: bool) {
    let macos = browser_runs_on_macos();
    let wanted = packed_replies_wanted(wire, emu, macos);
    lpa_link::device_link::wire_reader::set_packed_replies_wanted(wanted);
    match (wire, wanted) {
        (Some(WireChoice::Json), _) => {
            log::info!("dev flag: this page does not ask boards for packed replies (?wire=json)");
        }
        (Some(WireChoice::Packed), _) => {
            log::info!("dev flag: this page asks boards for packed replies (?wire=packed)");
        }
        (None, false) => log::info!(
            "Web Serial on macOS: boards reply in JSON, not packed — macOS Web Serial drops \
             bytes of packed replies (?wire=packed overrides)"
        ),
        (None, true) => {}
    }
}

/// `navigator.userAgentData.platform` where the browser has it (Chromium),
/// else `navigator.platform`.
#[cfg(target_arch = "wasm32")]
fn browser_runs_on_macos() -> bool {
    use wasm_bindgen::JsValue;
    let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
        return false;
    };
    let platform = js_sys::Reflect::get(&navigator, &JsValue::from_str("userAgentData"))
        .ok()
        .filter(|data| data.is_object())
        .and_then(|data| js_sys::Reflect::get(&data, &JsValue::from_str("platform")).ok())
        .and_then(|platform| platform.as_string())
        .filter(|platform| !platform.is_empty())
        .or_else(|| navigator.platform().ok())
        .unwrap_or_default();
    is_macos_platform(&platform)
}

/// `"macOS"` (userAgentData) or `"MacIntel"` (navigator.platform, Intel and
/// Apple silicon alike).
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm install; host builds only run the unit tests"
    )
)]
fn is_macos_platform(platform: &str) -> bool {
    platform == "macOS" || platform.starts_with("Mac")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_on_real_web_serial_asks_for_json() {
        assert!(!packed_replies_wanted(None, false, true));
    }

    #[test]
    fn everything_else_asks_for_packed() {
        assert!(packed_replies_wanted(None, false, false), "another OS");
        assert!(packed_replies_wanted(None, true, true), "a Mac on ?emu=");
    }

    #[test]
    fn the_wire_flag_overrides_the_default() {
        assert!(packed_replies_wanted(Some(WireChoice::Packed), false, true));
        assert!(!packed_replies_wanted(Some(WireChoice::Json), false, false));
        assert!(!packed_replies_wanted(Some(WireChoice::Json), true, true));
    }

    #[test]
    fn mac_platform_strings_are_recognised() {
        assert!(is_macos_platform("macOS"));
        assert!(is_macos_platform("MacIntel"));
        assert!(!is_macos_platform("Windows"));
        assert!(!is_macos_platform("Linux x86_64"));
        assert!(!is_macos_platform(""));
    }
}

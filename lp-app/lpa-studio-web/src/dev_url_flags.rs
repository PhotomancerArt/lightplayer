//! Dev-only URL flags that tune the device wire for a measurement, read once
//! at page load. None is a setting: no UI, no persistence, and a page
//! without them behaves exactly as shipped.
//!
//! | flag | what it does |
//! |---|---|
//! | `?lens-pause-ms=N` | the editor lens's pause between device reads (`DEVICE_REFRESH_INTERVAL`, 150 ms), clamped to 0–1000 ms — the JSON Pack cadence probe (plan `lp-json-pack`, D2) |
//! | `?wire=json` | this page does not ask boards to pack their replies, so JSON and packed can be measured on one build |
//! | `?wire-capture=1` | tee every raw byte the Web Serial read pump hands to Rust into a 16 MiB in-memory buffer; `lpWireCapture()` in the console downloads it as `wire-capture-<unix-ms>.bin` (`lpa_link::device_link::wire_capture`) |
//! | `?device-log=<level>` | once per link, after the board's hello and the packed-reply opt-in, ask it for `trace`/`debug`/`info`/`warn`/`error` logging (`SetLogLevel`) |
//!
//! Validated the way `?capture-sink=` is (`device_events_io.rs`): a query is
//! user input, a value that does not parse reads as no flag, and the page
//! says so once in the console. Both are documented beside `?emu=` in
//! `AGENTS.md` ("Studio against an emulated board").

use lpc_wire::server::api::LogLevel;

/// The dev flags a query string carries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm install; host builds only run the unit tests"
    )
)]
pub struct DevUrlFlags {
    /// `?lens-pause-ms=N`, unclamped (core clamps).
    pub lens_pause_ms: Option<u64>,
    /// `?wire=json`.
    pub wire_json: bool,
    /// `?wire-capture=1`.
    pub wire_capture: bool,
    /// `?device-log=<level>`.
    pub device_log: Option<LogLevel>,
    /// Flags present but unreadable, for the console.
    pub ignored: Vec<String>,
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm install; host builds only run the unit tests"
    )
)]
impl DevUrlFlags {
    /// Parse a `location.search` string (with or without the leading `?`).
    pub fn parse(search: &str) -> Self {
        let mut flags = Self::default();
        let query = search.strip_prefix('?').unwrap_or(search);
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "lens-pause-ms" => match value.trim().parse::<u64>() {
                    Ok(ms) => flags.lens_pause_ms = Some(ms),
                    Err(_) => flags.ignored.push(pair.to_string()),
                },
                "wire" => match value.trim() {
                    "json" => flags.wire_json = true,
                    // The default, spelled out: accepted, changes nothing.
                    "packed" => flags.wire_json = false,
                    _ => flags.ignored.push(pair.to_string()),
                },
                "wire-capture" => match value.trim() {
                    "1" | "true" | "" => flags.wire_capture = true,
                    "0" | "false" => flags.wire_capture = false,
                    _ => flags.ignored.push(pair.to_string()),
                },
                "device-log" => match parse_log_level(value.trim()) {
                    Some(level) => flags.device_log = Some(level),
                    None => flags.ignored.push(pair.to_string()),
                },
                _ => {}
            }
        }
        flags
    }
}

/// A `?device-log=` value, case-insensitive.
fn parse_log_level(value: &str) -> Option<LogLevel> {
    Some(match value.to_ascii_lowercase().as_str() {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => return None,
    })
}

/// Read the flags from this page's URL and apply them. Before any device
/// connects, so every reader built after it takes them.
#[cfg(target_arch = "wasm32")]
pub fn install() {
    let Some(search) = web_sys::window().and_then(|window| window.location().search().ok()) else {
        return;
    };
    let flags = DevUrlFlags::parse(&search);
    for pair in &flags.ignored {
        log::warn!("dev flag ignored (unreadable value): {pair}");
    }
    if let Some(ms) = flags.lens_pause_ms {
        let pause = lpa_studio_core::set_device_lens_pause_override(Some(ms));
        log::info!(
            "dev flag: the device lens pauses {} ms between reads",
            pause.as_millis()
        );
    }
    if flags.wire_json {
        lpa_link::device_link::wire_reader::set_packed_replies_wanted(false);
        log::info!("dev flag: this page does not ask boards for packed replies (?wire=json)");
    }
    if flags.wire_capture {
        install_wire_capture();
    }
    if let Some(level) = flags.device_log {
        lpa_link::device_link::wire_reader::set_device_log_level(Some(level));
        log::info!("dev flag: each board is asked for {level:?} logging once it is ready");
    }
}

/// Start the raw-byte tee and put `window.lpWireCapture()` on the page: it
/// downloads everything captured so far as `wire-capture-<unix-ms>.bin`, and
/// returns the byte count. The capture keeps running.
#[cfg(target_arch = "wasm32")]
fn install_wire_capture() {
    use wasm_bindgen::prelude::*;

    lpa_link::device_link::wire_capture::set_wire_capture(true);
    let download = Closure::<dyn Fn() -> f64>::new(|| {
        let bytes = lpa_link::device_link::wire_capture::wire_capture_bytes();
        let name = format!("wire-capture-{}.bin", js_sys::Date::now() as u64);
        if let Err(error) = crate::app::home::package_export::trigger_download(
            &name,
            "application/octet-stream",
            &bytes,
        ) {
            log::warn!("wire capture download failed: {error:?}");
        }
        bytes.len() as f64
    });
    let installed = web_sys::window().is_some_and(|window| {
        js_sys::Reflect::set(&window, &"lpWireCapture".into(), download.as_ref()).is_ok()
    });
    // The page lives as long as the function does.
    download.forget();
    if installed {
        log::info!(
            "dev flag: capturing the device port's raw bytes (?wire-capture=1); \
             run lpWireCapture() in the console to download them"
        );
    }
}

/// Host builds read no URL.
#[cfg(not(target_arch = "wasm32"))]
pub fn install() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_flags_parse_beside_other_query_keys() {
        let flags = DevUrlFlags::parse("?emu=tab&lens-pause-ms=75&wire=json&on=emu");
        assert_eq!(flags.lens_pause_ms, Some(75));
        assert!(flags.wire_json);
        assert!(flags.ignored.is_empty());
    }

    #[test]
    fn the_capture_and_log_flags_parse() {
        let flags = DevUrlFlags::parse("?emu=ws://127.0.0.1:9/&wire-capture=1&device-log=Debug");
        assert!(flags.wire_capture);
        assert_eq!(flags.device_log, Some(LogLevel::Debug));
        assert!(flags.ignored.is_empty());

        let flags = DevUrlFlags::parse("wire-capture=yes&device-log=loud");
        assert!(!flags.wire_capture);
        assert_eq!(flags.device_log, None);
        assert_eq!(flags.ignored, ["wire-capture=yes", "device-log=loud"]);
    }

    #[test]
    fn no_flags_is_the_shipped_page() {
        assert_eq!(DevUrlFlags::parse(""), DevUrlFlags::default());
        assert_eq!(DevUrlFlags::parse("?emu=tab"), DevUrlFlags::default());
    }

    #[test]
    fn unreadable_values_are_ignored_and_named() {
        let flags = DevUrlFlags::parse("lens-pause-ms=fast&wire=ion");
        assert_eq!(flags.lens_pause_ms, None);
        assert!(!flags.wire_json);
        assert_eq!(flags.ignored, ["lens-pause-ms=fast", "wire=ion"]);
    }
}

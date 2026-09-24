//! Dev-only URL flags that tune the device wire for a measurement, read once
//! at page load. Neither is a setting: no UI, no persistence, and a page
//! without them behaves exactly as shipped.
//!
//! | flag | what it does |
//! |---|---|
//! | `?lens-pause-ms=N` | the editor lens's pause between device reads (`DEVICE_REFRESH_INTERVAL`, 150 ms), clamped to 0–1000 ms — the JSON Pack cadence probe (plan `lp-json-pack`, D2) |
//! | `?wire=json` | this page does not ask boards to pack their replies, so JSON and packed can be measured on one build |
//!
//! Validated the way `?capture-sink=` is (`device_events_io.rs`): a query is
//! user input, a value that does not parse reads as no flag, and the page
//! says so once in the console. Both are documented beside `?emu=` in
//! `AGENTS.md` ("Studio against an emulated board").

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
                _ => {}
            }
        }
        flags
    }
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

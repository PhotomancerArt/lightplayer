//! Dev-only URL flags that tune the device wire for a measurement, read once
//! at page load. None is a setting: no UI, no persistence, and a page
//! without them behaves exactly as shipped.
//!
//! | flag | what it does |
//! |---|---|
//! | `?lens-pause-ms=N` | the editor lens's pause between device reads (`DEVICE_REFRESH_INTERVAL`, 150 ms), clamped to 0–1000 ms — the JSON Pack cadence probe (plan `lp-json-pack`, D2) |
//! | `?wire=json` / `?wire=packed` | whether this page asks boards to pack their replies (the default is packed, everywhere), so JSON and packed can be measured on one build |
//! | `?wire-capture=1` | tee every raw byte the Web Serial read pump hands to Rust into a 16 MiB in-memory buffer; `lpWireCapture()` in the console downloads it as `wire-capture-<unix-ms>.bin` (`lpa_link::device_link::wire_capture`) |
//! | `?device-log=<level>` | once per link, after the board's hello and the packed-reply opt-in, ask it for `trace`/`debug`/`info`/`warn`/`error` logging (`SetLogLevel`) |
//! | `?lan=<url>[,<url>…]` | a dev shortcut: dial each named board on the LAN (`ws://<board>/link`, or just its host) at page load, over a secure lp-link, as a Wi-Fi device on the Devices page. Wi-Fi boards need no flag — a board Studio has met is offered "Connect over Wi‑Fi", and the add slot takes an address — this only saves the typing (Wi-Fi M6 P07, network transport P01; parsed by `lpa_studio_core::parse_lan_flag`) |
//! | `?firmware-store=<origin>` | the firmware store Studio fetches engines from, instead of `https://lightplayer.app` — **loopback and private-LAN origins only** (the `?record=` sink rule, `record_sink::check_sink`), so a link someone else wrote cannot point Studio at another store's "latest"; a refused origin keeps the default and says so once in the console |
//! | `?seams=<atoms\|none>` | what a **Devices-page** emulated board asks the emulator for, instead of the end-user default `led=fast` (`lpa_link::providers::emulator_tab_seams`); `none` is today's seam-free machine, for an A/B on one build. Never reaches `?emu=tab` / `?emu=ws://…` boards, which ask for nothing |
//!
//! Validated the way `?record=` is (`device_events_io.rs`): a query is
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
    /// `?wire=json` / `?wire=packed`; `None` takes the default (packed).
    pub wire: Option<WireChoice>,
    /// `?wire-capture=1`.
    pub wire_capture: bool,
    /// `?device-log=<level>`.
    pub device_log: Option<LogLevel>,
    /// `?firmware-store=<origin>`, judged.
    pub firmware_store: Option<FirmwareStoreFlag>,
    /// `?lan=<url>[,<url>…]`: the boards' sockets, normalised.
    pub lan: Vec<String>,
    /// `?seams=<atoms|none>`, normalized (`led=fast`, `led=fast+x=y`,
    /// `none`).
    pub seams: Option<String>,
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
                    "json" => flags.wire = Some(WireChoice::Json),
                    "packed" => flags.wire = Some(WireChoice::Packed),
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
                "seams" => match parse_seams(value) {
                    Some(seams) => flags.seams = Some(seams),
                    None => flags.ignored.push(pair.to_string()),
                },
                "firmware-store" if !value.trim().is_empty() => {
                    flags.firmware_store = Some(judge_firmware_store(value.trim()));
                }
                "lan" => {
                    let lan = lpa_studio_core::parse_lan_flag(value);
                    for (refused, why) in lan.refused {
                        flags.ignored.push(format!("lan={refused} ({why})"));
                    }
                    for address in lan.addresses {
                        if !flags.lan.contains(&address) {
                            flags.lan.push(address);
                        }
                    }
                }
                _ => {}
            }
        }
        flags
    }
}

/// A `?wire=` value: the reply encoding the page asks boards for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireChoice {
    Json,
    Packed,
}

/// What became of a `?firmware-store=` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FirmwareStoreFlag {
    /// Use this origin (`http(s)://host[:port]`, no path).
    Accepted(String),
    /// Keep the default; `reason` is for the console.
    Refused { value: String, reason: String },
}

/// Judge a (still percent-encoded) `?firmware-store=` value: an `http(s)`
/// origin — no path, query or credentials — whose host passes exactly the
/// check `?record=` applies to its sink (loopback, RFC 1918, `*.local`).
pub fn judge_firmware_store(raw: &str) -> FirmwareStoreFlag {
    let value = percent_decode(raw);
    let refuse = |reason: &str| FirmwareStoreFlag::Refused {
        value: value.clone(),
        reason: reason.to_string(),
    };
    let Some((scheme, rest)) = value.split_once("://") else {
        return refuse("not an http(s) origin");
    };
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if authority.is_empty() || authority.contains(['/', '?', '#', '@', ' ']) {
        return refuse("an origin is scheme://host[:port], with no path, query or credentials");
    }
    let host = authority.to_ascii_lowercase();
    let hostname = if host.starts_with('[') {
        match host.find(']') {
            Some(end) => host[..=end].to_string(),
            None => return refuse("an unterminated IPv6 address"),
        }
    } else {
        match host.rsplit_once(':') {
            Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name.to_string(),
            Some(_) => return refuse("a port must be a number"),
            None => host.clone(),
        }
    };
    match crate::record_sink::check_sink(&format!("{scheme}:"), &hostname, &host) {
        crate::record_sink::SinkCheck::Accepted { host } => {
            FirmwareStoreFlag::Accepted(format!("{scheme}://{host}"))
        }
        crate::record_sink::SinkCheck::Refused { reason, .. } => refuse(&reason),
    }
}

/// `%XX` escapes decoded (a query value is percent-encoded).
/// A `?seams=` value, normalized: `none`, or `name=impl` atoms joined by `+`
/// (a raw `+` in a query can arrive as a space, so either joins). Lowercase
/// letters, digits and `_` only — the emulator owns which atoms exist and
/// says so on the board if one does not; this only refuses what cannot be
/// one.
fn parse_seams(raw: &str) -> Option<String> {
    let value = percent_decode(raw).trim().replace(' ', "+");
    if value == "none" {
        return Some(value);
    }
    let word = |w: &str| {
        !w.is_empty()
            && w.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    let atoms: Vec<&str> = value.split('+').collect();
    let valid = atoms.iter().all(|atom| {
        atom.split_once('=')
            .is_some_and(|(name, imp)| word(name) && word(imp))
    });
    (valid && !atoms.is_empty()).then_some(value)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = value.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The firmware store origin this page uses: the accepted
/// `?firmware-store=` value, else `https://lightplayer.app`. Read once by
/// [`install`]; the shell asks for it when it builds the store client.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm install; host builds only run the unit tests"
    )
)]
pub fn firmware_store_origin() -> String {
    FIRMWARE_STORE_ORIGIN
        .with(|origin| origin.borrow().clone())
        .unwrap_or_else(|| lpa_firmware_store::DEFAULT_FIRMWARE_STORE_ORIGIN.to_string())
}

thread_local! {
    static FIRMWARE_STORE_ORIGIN: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    static LAN_ADDRESSES: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The boards `?lan=` named, as the sockets Studio dials at page load. Read
/// once by [`install`]; empty without the flag (the LAN transport is
/// installed either way: Wi‑Fi boards need no flag).
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm install; host builds only run the unit tests"
    )
)]
pub fn lan_addresses() -> Vec<String> {
    LAN_ADDRESSES.with(|addresses| addresses.borrow().clone())
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
    match flags.wire {
        Some(WireChoice::Json) => {
            lpa_link::device_link::wire_reader::set_packed_replies_wanted(false);
            log::info!("dev flag: this page does not ask boards for packed replies (?wire=json)");
        }
        // The default, spelled out: accepted, changes nothing.
        Some(WireChoice::Packed) => {
            lpa_link::device_link::wire_reader::set_packed_replies_wanted(true);
        }
        None => {}
    }
    if flags.wire_capture {
        install_wire_capture();
    }
    if let Some(level) = flags.device_log {
        lpa_link::device_link::wire_reader::set_device_log_level(Some(level));
        log::info!("dev flag: each board is asked for {level:?} logging once it is ready");
    }
    if !flags.lan.is_empty() {
        log::info!(
            "dev flag: reaching {} over Wi-Fi (?lan=)",
            flags.lan.join(", ")
        );
        LAN_ADDRESSES.with(|slot| *slot.borrow_mut() = flags.lan.clone());
    }
    if let Some(seams) = flags.seams {
        log::info!("dev flag: Devices-page emulated boards ask for seams `{seams}` (?seams=)");
        lpa_link::providers::emulator_tab_seams::set_end_user_seams_override(Some(seams));
    }
    match flags.firmware_store {
        Some(FirmwareStoreFlag::Accepted(origin)) => {
            log::info!("dev flag: firmware store at {origin} (?firmware-store=)");
            FIRMWARE_STORE_ORIGIN.with(|slot| *slot.borrow_mut() = Some(origin));
        }
        Some(FirmwareStoreFlag::Refused { value, reason }) => {
            log::warn!(
                "dev flag ?firmware-store={value} refused ({reason}); the firmware store stays {}",
                lpa_firmware_store::DEFAULT_FIRMWARE_STORE_ORIGIN
            );
        }
        None => {}
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
        assert_eq!(flags.wire, Some(WireChoice::Json));
        assert!(flags.ignored.is_empty());

        let flags = DevUrlFlags::parse("wire=packed");
        assert_eq!(flags.wire, Some(WireChoice::Packed));
    }

    #[test]
    fn the_seams_flag_parses_and_refuses_what_cannot_be_an_atom() {
        assert_eq!(
            DevUrlFlags::parse("?seams=none").seams.as_deref(),
            Some("none")
        );
        assert_eq!(
            DevUrlFlags::parse("?emu=tab&seams=led=fast")
                .seams
                .as_deref(),
            Some("led=fast")
        );
        assert_eq!(
            DevUrlFlags::parse("seams=led%3Dfast%2Btest%3Decho")
                .seams
                .as_deref(),
            Some("led=fast+test=echo")
        );
        assert_eq!(
            DevUrlFlags::parse("seams=led=fast test=echo")
                .seams
                .as_deref(),
            Some("led=fast+test=echo"),
            "a raw `+` that arrived as a space"
        );
        let flags = DevUrlFlags::parse("seams=led&seams=LED=Fast");
        assert_eq!(flags.seams, None);
        assert_eq!(flags.ignored, ["seams=led", "seams=LED=Fast"]);
        assert_eq!(
            DevUrlFlags::parse("emu=tab").seams,
            None,
            "no flag, no override"
        );
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
    fn the_lan_flag_names_each_board_and_refuses_what_is_not_one() {
        let flags = DevUrlFlags::parse(
            "?emu=tab&lan=ws%3A%2F%2F10.0.0.5%2Flink,lp-b48c.local,http://x/&wire=json",
        );
        assert_eq!(flags.lan, ["ws://10.0.0.5/link", "ws://lp-b48c.local/link"]);
        assert_eq!(flags.wire, Some(WireChoice::Json));
        assert_eq!(flags.ignored.len(), 1, "{:?}", flags.ignored);
        assert!(
            flags.ignored[0].starts_with("lan=http://x/"),
            "{:?}",
            flags.ignored
        );
    }

    #[test]
    fn no_flags_is_the_shipped_page() {
        assert_eq!(DevUrlFlags::parse(""), DevUrlFlags::default());
        assert_eq!(DevUrlFlags::parse("?on=mac:aa"), DevUrlFlags::default());
        assert_eq!(DevUrlFlags::parse("?emu=tab"), DevUrlFlags::default());
    }

    #[test]
    fn local_firmware_store_origins_are_accepted() {
        for (raw, origin) in [
            ("http://127.0.0.1:2812", "http://127.0.0.1:2812"),
            ("http%3A%2F%2F127.0.0.1%3A2812%2F", "http://127.0.0.1:2812"),
            ("http://localhost:31415", "http://localhost:31415"),
            ("HTTP://LocalHost:9", "http://localhost:9"),
            ("https://192.168.1.20", "https://192.168.1.20"),
            ("http://10.0.0.5:8080", "http://10.0.0.5:8080"),
            (
                "http://studio-mac.local:2812",
                "http://studio-mac.local:2812",
            ),
            ("http://[::1]:2812", "http://[::1]:2812"),
        ] {
            assert_eq!(
                judge_firmware_store(raw),
                FirmwareStoreFlag::Accepted(origin.to_string()),
                "{raw}"
            );
        }
        let flags = DevUrlFlags::parse("?emu=tab&firmware-store=http%3A%2F%2F127.0.0.1%3A2812");
        assert_eq!(
            flags.firmware_store,
            Some(FirmwareStoreFlag::Accepted("http://127.0.0.1:2812".into()))
        );
    }

    #[test]
    fn other_firmware_store_values_are_refused() {
        for raw in [
            "https://lightplayer.app",
            "https://evil.example",
            "https%3A%2F%2Fevil.example",
            "http://8.8.8.8",
            "http://127.0.0.1.nip.io",
            "http://localhost.evil.example",
            "ftp://127.0.0.1",
            "127.0.0.1:2812",
            "http://127.0.0.1:2812/firmware",
            "http://127.0.0.1:2812?x=1",
            "http://user@127.0.0.1",
            "http://127.0.0.1:port",
            "http://",
            "http://[::1",
        ] {
            assert!(
                matches!(judge_firmware_store(raw), FirmwareStoreFlag::Refused { .. }),
                "{raw}"
            );
        }
        assert_eq!(DevUrlFlags::parse("?firmware-store=").firmware_store, None);
    }

    #[test]
    fn the_default_origin_is_lightplayer_app() {
        assert_eq!(firmware_store_origin(), "https://lightplayer.app");
    }

    #[test]
    fn unreadable_values_are_ignored_and_named() {
        let flags = DevUrlFlags::parse("lens-pause-ms=fast&wire=ion");
        assert_eq!(flags.lens_pause_ms, None);
        assert_eq!(flags.wire, None);
        assert_eq!(flags.ignored, ["lens-pause-ms=fast", "wire=ion"]);
    }
}

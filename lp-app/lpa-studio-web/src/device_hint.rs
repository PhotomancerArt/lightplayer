//! `?on=` — the device hint that rides a project address.
//!
//! The URL is the PROJECT (vision D35): `/p/<slug>-prj<uid>` is the whole
//! of the identity and the whole of the share link. The device is a
//! **hint** on top of it — enough for a reload to land back on the thing
//! that was running a moment ago, and never enough to change what the
//! address means. Drop the hint and you still have the project; drop the
//! project and you have nothing.
//!
//! The grammar (vision D43) takes a **kind** or an **instance**:
//!
//! | hint | means |
//! |---|---|
//! | `?on=emu` | the emu for the project's target — the real firmware binary in `lp-emu` |
//! | `?on=sim` | the sim for the project's target — `fw-browser` wearing its manifest |
//! | `?on=mac:60:55:f9:0a:0b:0c` | a board by identity: silicon on the desk, or an emu by its synthetic MAC |
//! | `?on=ws:192.168.0.21:1234` | a device over WebSocket: a real desktop server, or a native emu backing |
//!
//! A kind resolves to *a* device (the one that last ran this project, else
//! an idle one of the target, else a fresh one); an instance names one
//! device and nothing else. What each of them does on arrival is
//! `web_app.rs`'s business (PD14) — this file only spells them.
//!
//! **Unknown hints read as no hint.** The query is user input, exactly
//! like the path, and the router's rule for user input is to land
//! somewhere honest rather than to guess: `?on=banana` opens the project
//! on the default device, and the address is rewritten without the hint.

/// Which device a project address asks to run on (D43).
///
/// `Display` emits exactly what [`DeviceHint::parse`] reads, so an address
/// survives every round trip through the router.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "constructed by the wasm route listeners; host builds only run the unit tests"
    )
)]
pub(crate) enum DeviceHint {
    /// `?on=emu` — the emu of the project's target. No emulator module
    /// ships in this build (D48), so the arrival says so and falls back to
    /// a sim; the hint parses today so the addresses people write down
    /// keep working when one lands.
    Emu,
    /// `?on=sim` — the sim of the project's target.
    Sim,
    /// `?on=mac:<mac>` — a board by identity, in the one spelling identity
    /// uses (lowercase colon hex). Silicon or a sim: a MAC names a device,
    /// not a kind.
    Mac(String),
    /// `?on=ws:<host>:<port>` — a device reached over WebSocket. Parsed
    /// and reported; nothing in this build backs it (Q9).
    Ws(String),
}

impl DeviceHint {
    /// The hint an `on=` query value spells, or `None` when it spells
    /// nothing this router knows.
    ///
    /// A malformed MAC is `None` rather than `Mac("banana")`: a hint that
    /// can never resolve is not a hint, and letting it through would put an
    /// address in the bar that no device could ever answer.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(mac) = value.strip_prefix("mac:") {
            return lpa_link::normalize_base_mac(mac).map(DeviceHint::Mac);
        }
        if let Some(address) = value.strip_prefix("ws:") {
            // Host and port are the server's business, not ours — this
            // build has nothing to connect them to. An empty address is
            // still no address.
            return (!address.trim().is_empty()).then(|| DeviceHint::Ws(address.trim().to_string()));
        }
        match value {
            "emu" => Some(DeviceHint::Emu),
            "sim" => Some(DeviceHint::Sim),
            _ => None,
        }
    }
}

impl core::fmt::Display for DeviceHint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeviceHint::Emu => f.write_str("emu"),
            DeviceHint::Sim => f.write_str("sim"),
            DeviceHint::Mac(mac) => write!(f, "mac:{mac}"),
            DeviceHint::Ws(address) => write!(f, "ws:{address}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hint_round_trips_through_its_spelling() {
        for hint in [
            DeviceHint::Emu,
            DeviceHint::Sim,
            DeviceHint::Mac("60:55:f9:0a:0b:0c".to_string()),
            DeviceHint::Ws("192.168.0.21:1234".to_string()),
        ] {
            let spelled = hint.to_string();
            assert_eq!(DeviceHint::parse(&spelled), Some(hint), "{spelled:?}");
        }
    }

    /// A MAC is the same address however it is written down; the STORED
    /// form is canonical (`lpa_link::normalize_base_mac`'s rule, reused
    /// rather than re-decided).
    #[test]
    fn a_mac_hint_normalizes_to_lowercase_colon_hex() {
        assert_eq!(
            DeviceHint::parse("mac:60:55:F9:0A:0B:0C"),
            Some(DeviceHint::Mac("60:55:f9:0a:0b:0c".to_string()))
        );
        assert_eq!(
            DeviceHint::parse("mac:60:55:F9:0A:0B:0C").map(|hint| hint.to_string()),
            Some("mac:60:55:f9:0a:0b:0c".to_string())
        );
    }

    /// The query is user input: a hint nothing could ever answer to is no
    /// hint at all, and the arrival opens the project on its default
    /// device.
    #[test]
    fn junk_is_no_hint() {
        for junk in [
            "",
            " ",
            "banana",
            "SIM",
            "mac:",
            "mac:banana",
            // an EUI-64 is eight octets: accepting one would mint a second
            // identity for the same board
            "mac:60:55:f9:ff:fe:0a:0b:0c",
            "mac:00:00:00:00:00:00",
            "ws:",
            "ws: ",
        ] {
            assert_eq!(DeviceHint::parse(junk), None, "{junk:?}");
        }
    }
}

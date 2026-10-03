//! [`BoardRef`]: a board's segment in an offer path, which says what KIND
//! of id it is as well as the id — `mac-a0f26287b48c`, `sim-…`, `emu-…`,
//! or `new-1`.
//!
//! The MAC alone cannot tell a sim from silicon: every made board mints a
//! locally administered MAC ([`BoardKey::locally_administered`]), and so
//! can a board served by `lp-cli emu serve`, which Studio reaches through
//! the virtual serial shim exactly as it reaches a real one. What does tell
//! them apart is where the link is reached — the endpoint scheme
//! ([`SIM_ENDPOINT_PREFIX`], [`EMU_ENDPOINT_PREFIX`]) — and those schemes
//! are studio-core's: the device model never reads them, so the kind is
//! decided here and not in `lpa-devices`.

use core::fmt;
use core::str::FromStr;

use lpa_devices::BoardKey;
use lpa_devices::identity::IdentityChain;

use super::sim_record::{EMU_ENDPOINT_PREFIX, SIM_ENDPOINT_PREFIX};

/// One board's id in an offer path, kind first:
///
/// - `mac-<12 hex>`: a board known by its silicon MAC — a real board, and
///   also one `lp-cli emu serve` holds (Studio sees it like a real board);
/// - `sim-<12 hex>`: an in-browser sim (a `sim:` endpoint), by its minted MAC;
/// - `emu-<12 hex>`: an in-tab emulated board (an `emu:` endpoint), by its
///   minted MAC;
/// - `new-<n>`: a link or device that has not said who it is yet, by a
///   small number it keeps while it stays that way
///   ([`super::ProvisionalBoardNumbers`]: `new-1` with one such board on the
///   desk). It changes to one of the others once it says who it is.
///
/// No segment contains a `.` (an offer path's node mark) or a `/`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoardRef {
    /// A board known by its silicon MAC.
    Mac(BoardKey),
    /// An in-browser sim, by its minted MAC.
    Sim(BoardKey),
    /// An in-tab emulated board, by its minted MAC.
    Emu(BoardKey),
    /// Not identified yet, by its provisional number
    /// ([`super::ProvisionalBoardNumbers`]) — not the roster's handle,
    /// which can be a 19-digit id.
    New(u32),
}

impl BoardRef {
    const MAC: &'static str = "mac-";
    const SIM: &'static str = "sim-";
    const EMU: &'static str = "emu-";
    const NEW: &'static str = "new-";

    /// The ref a device or pending link with this identity goes by once
    /// it has a MAC: the MAC with the kind its endpoint names. `None`
    /// while it has no MAC — it is offered as `new-<n>` then, by the number
    /// [`super::ProvisionalBoardNumbers`] gives it. An endpoint that is
    /// neither `sim:` nor `emu:` — a serial port, Bluetooth, or none
    /// recorded — is a `mac-` board.
    pub fn known(identity: &IdentityChain) -> Option<Self> {
        let key = identity.mac.as_ref().and_then(BoardKey::from_mac)?;
        Some(Self::for_endpoint(
            key,
            identity
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.0.as_str()),
        ))
    }

    /// A known board's ref, its kind read off the endpoint it is reached at.
    pub fn for_endpoint(key: BoardKey, endpoint: Option<&str>) -> Self {
        match endpoint {
            Some(endpoint) if endpoint.starts_with(SIM_ENDPOINT_PREFIX) => Self::Sim(key),
            Some(endpoint) if endpoint.starts_with(EMU_ENDPOINT_PREFIX) => Self::Emu(key),
            _ => Self::Mac(key),
        }
    }

    /// Read a segment back: `mac-a0f26287b48c`, `sim-…`, `emu-…`, `new-1`.
    /// The MAC part must be the canonical 12 lowercase hex, the number a
    /// canonical decimal from 1.
    pub fn parse(segment: &str) -> Result<Self, BoardRefError> {
        let refused = || BoardRefError(segment.to_string());
        if let Some(n) = segment.strip_prefix(Self::NEW) {
            let number = n
                .parse::<u32>()
                .ok()
                .filter(|number| *number >= 1 && number.to_string() == n);
            return number.map(Self::New).ok_or_else(refused);
        }
        let (make, hex): (fn(BoardKey) -> Self, &str) =
            if let Some(hex) = segment.strip_prefix(Self::MAC) {
                (Self::Mac, hex)
            } else if let Some(hex) = segment.strip_prefix(Self::SIM) {
                (Self::Sim, hex)
            } else if let Some(hex) = segment.strip_prefix(Self::EMU) {
                (Self::Emu, hex)
            } else {
                return Err(refused());
            };
        let key = BoardKey::parse(hex).map_err(|_| refused())?;
        if key.to_string() != hex {
            return Err(refused());
        }
        Ok(make(key))
    }

    /// The board's MAC, or `None` for a `new-<n>` ref.
    pub fn key(&self) -> Option<BoardKey> {
        match self {
            Self::Mac(key) | Self::Sim(key) | Self::Emu(key) => Some(*key),
            Self::New(_) => None,
        }
    }
}

impl fmt::Display for BoardRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mac(key) => write!(f, "{}{key}", Self::MAC),
            Self::Sim(key) => write!(f, "{}{key}", Self::SIM),
            Self::Emu(key) => write!(f, "{}{key}", Self::EMU),
            Self::New(number) => write!(f, "{}{number}", Self::NEW),
        }
    }
}

impl FromStr for BoardRef {
    type Err = BoardRefError;

    fn from_str(segment: &str) -> Result<Self, Self::Err> {
        Self::parse(segment)
    }
}

/// A segment that is not a [`BoardRef`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardRefError(pub String);

impl fmt::Display for BoardRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a board id (mac-, sim- or emu- and 12 lowercase hex, or new-<n> from 1)",
            self.0
        )
    }
}

impl std::error::Error for BoardRefError {}

#[cfg(test)]
mod tests {
    use lpa_devices::identity::{EndpointKey, MacAddress};

    use super::*;
    use crate::app::devices::sim_record::{ble_endpoint, emu_endpoint, sim_endpoint};

    #[test]
    fn the_endpoint_decides_the_kind_and_the_mac_the_id() {
        let chain = |endpoint: Option<EndpointKey>, mac: &str| IdentityChain {
            endpoint,
            mac: Some(MacAddress(mac.to_string())),
            ..IdentityChain::default()
        };
        let cases = [
            (
                chain(Some(EndpointKey("usb-1".into())), "a0:f2:62:87:b4:8c"),
                "mac-a0f26287b48c",
            ),
            (
                chain(Some(ble_endpoint("C31ED9CA")), "a0:f2:62:87:b4:8c"),
                "mac-a0f26287b48c",
            ),
            (chain(None, "a0:f2:62:87:b4:8c"), "mac-a0f26287b48c"),
            (
                chain(Some(sim_endpoint("dev_sim")), "12:22:33:44:55:66"),
                "sim-122233445566",
            ),
            (
                chain(Some(emu_endpoint("dev_emu")), "12:22:33:44:55:66"),
                "emu-122233445566",
            ),
        ];
        for (identity, expected) in cases {
            let board = BoardRef::known(&identity).expect("a MAC is known");
            assert_eq!(board.to_string(), expected, "{identity:?}");
            assert_eq!(BoardRef::parse(expected), Ok(board), "round-trips");
        }

        let anonymous = IdentityChain {
            endpoint: Some(sim_endpoint("dev_sim")),
            ..IdentityChain::default()
        };
        assert_eq!(
            BoardRef::known(&anonymous),
            None,
            "no MAC yet is provisional, whatever the endpoint"
        );
        assert_eq!(BoardRef::New(3).to_string(), "new-3");
        assert_eq!(BoardRef::parse("new-3"), Ok(BoardRef::New(3)));
    }

    #[test]
    fn only_canonical_segments_parse() {
        for segment in [
            "",
            "a0f26287b48c",
            "mac-",
            "mac-a0:f2:62:87:b4:8c",
            "mac-A0F26287B48C",
            "mac-000000000000",
            "usb-a0f26287b48c",
            "new-",
            "new-x",
            "new-+3",
            "new-03",
            "new-0",
            "new-9223372036854775809",
        ] {
            assert!(BoardRef::parse(segment).is_err(), "{segment}");
        }
    }
}

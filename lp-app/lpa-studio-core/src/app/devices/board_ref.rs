//! [`BoardRef`]: a board's segment in an offer path, which says what KIND
//! of id it is as well as the id — `mac-a0f26287b48c`, `sim-…`, `emu-…`,
//! or `new-3`.
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

use lpa_devices::identity::IdentityChain;
use lpa_devices::{BoardKey, DeviceId};

use super::sim_record::{EMU_ENDPOINT_PREFIX, SIM_ENDPOINT_PREFIX};

/// One board's id in an offer path, kind first:
///
/// - `mac-<12 hex>`: a board known by its silicon MAC — a real board, and
///   also one `lp-cli emu serve` holds (Studio sees it like a real board);
/// - `sim-<12 hex>`: an in-browser sim (a `sim:` endpoint), by its minted MAC;
/// - `emu-<12 hex>`: an in-tab emulated board (an `emu:` endpoint), by its
///   minted MAC;
/// - `new-<n>`: a link or device that has not said who it is yet, by its
///   roster handle. It changes to one of the others once it does.
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
    /// Not identified yet: the roster's provisional handle.
    New(DeviceId),
}

impl BoardRef {
    const MAC: &'static str = "mac-";
    const SIM: &'static str = "sim-";
    const EMU: &'static str = "emu-";
    const NEW: &'static str = "new-";

    /// The ref a device or pending link with this identity goes by: its MAC
    /// with the kind its endpoint names, or `new-<provisional>` while it has
    /// no MAC. An endpoint that is neither `sim:` nor `emu:` — a serial
    /// port, Bluetooth, or none recorded — is a `mac-` board.
    pub fn for_identity(identity: &IdentityChain, provisional: DeviceId) -> Self {
        match identity.mac.as_ref().and_then(BoardKey::from_mac) {
            Some(key) => Self::for_endpoint(
                key,
                identity
                    .endpoint
                    .as_ref()
                    .map(|endpoint| endpoint.0.as_str()),
            ),
            None => Self::New(provisional),
        }
    }

    /// A known board's ref, its kind read off the endpoint it is reached at.
    pub fn for_endpoint(key: BoardKey, endpoint: Option<&str>) -> Self {
        match endpoint {
            Some(endpoint) if endpoint.starts_with(SIM_ENDPOINT_PREFIX) => Self::Sim(key),
            Some(endpoint) if endpoint.starts_with(EMU_ENDPOINT_PREFIX) => Self::Emu(key),
            _ => Self::Mac(key),
        }
    }

    /// Read a segment back: `mac-a0f26287b48c`, `sim-…`, `emu-…`, `new-3`.
    /// The MAC part must be the canonical 12 lowercase hex.
    pub fn parse(segment: &str) -> Result<Self, BoardRefError> {
        let refused = || BoardRefError(segment.to_string());
        if let Some(n) = segment.strip_prefix(Self::NEW) {
            let id = n.parse::<u64>().ok().filter(|id| id.to_string() == n);
            return id.map(|id| Self::New(DeviceId(id))).ok_or_else(refused);
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
            Self::New(id) => write!(f, "{}{}", Self::NEW, id.0),
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
            "`{}` is not a board id (mac-, sim- or emu- and 12 lowercase hex, or new-<n>)",
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
            let board = BoardRef::for_identity(&identity, DeviceId(3));
            assert_eq!(board.to_string(), expected, "{identity:?}");
            assert_eq!(BoardRef::parse(expected), Ok(board), "round-trips");
        }

        let anonymous = IdentityChain {
            endpoint: Some(sim_endpoint("dev_sim")),
            ..IdentityChain::default()
        };
        assert_eq!(
            BoardRef::for_identity(&anonymous, DeviceId(3)).to_string(),
            "new-3",
            "no MAC yet is provisional, whatever the endpoint"
        );
        assert_eq!(BoardRef::parse("new-3"), Ok(BoardRef::New(DeviceId(3))));
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
        ] {
            assert!(BoardRef::parse(segment).is_err(), "{segment}");
        }
    }
}

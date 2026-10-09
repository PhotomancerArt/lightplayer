//! [`HoldKey`]: which board, by which way in, a tab holds — and the Web Lock
//! name that says so.
//!
//! The lock is taken AFTER the board's hello (Web Serial gives a page no
//! serial number, so the MAC is unknown until the port is open and the board
//! has spoken): `lp-board:usb:<vid>:<pid>:<mac>` for a USB port,
//! `lp-board:net:<mac>` for the board's one network slot. Lowercase hex,
//! the MAC without separators. One spelling only, so one board's lock is
//! one name in every tab.
//!
//! Not persisted: a lock name lives as long as the tab that holds it.

use core::fmt;

use lpa_devices::link::LinkInfo;
use lpa_devices::{BoardKey, HoldVia, MacAddress};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Every board hold's Web Lock name starts with this. The library's own
/// locks (`lp-project:<uid>`, `lp-catalog`) never do.
pub const LOCK_PREFIX: &str = "lp-board:";

const USB_SEGMENT: &str = "usb";
const NETWORK_SEGMENT: &str = "net";

/// A board held by one tab, on one way in.
///
/// A USB hold always names its port's vendor and product (what another
/// tab can see of a port it has not opened); a network hold never does.
/// The constructors keep that true, so the fields are private.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HoldKey {
    via: HoldVia,
    mac: BoardKey,
    usb: Option<UsbPair>,
}

/// A USB port's vendor and product id, as `getInfo()` reports them.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UsbPair {
    pub vendor: u16,
    pub product: u16,
}

impl HoldKey {
    /// The board's USB port, reached through a port with this vendor and
    /// product.
    pub fn usb(mac: BoardKey, pair: UsbPair) -> Self {
        Self {
            via: HoldVia::Usb,
            mac,
            usb: Some(pair),
        }
    }

    /// The board's one network slot (the LAN or the cloud relay).
    pub fn network(mac: BoardKey) -> Self {
        Self {
            via: HoldVia::Network,
            mac,
            usb: None,
        }
    }

    /// The hold a link to the board with `mac` is: its USB port's, or its
    /// network slot's (the LAN or the relay: one slot, whichever road took
    /// it). `None` for a link no tab holds by this protocol: Bluetooth, a
    /// sim, an in-tab emulated board, a port with no vendor and product.
    pub fn of_link(mac: BoardKey, info: &LinkInfo) -> Option<Self> {
        if let Some(pair) = super::hold_gate::usb_pair_of(info) {
            return Some(Self::usb(mac, pair));
        }
        is_network_road(info).then(|| Self::network(mac))
    }

    pub fn via(&self) -> HoldVia {
        self.via
    }

    pub fn mac(&self) -> BoardKey {
        self.mac
    }

    /// The MAC in the spelling the device model addresses boards by
    /// (`a0:f2:62:87:b4:8c`), for `Event::BoardHeld`.
    pub fn mac_address(&self) -> MacAddress {
        self.mac.to_mac_address()
    }

    /// The port's vendor and product, for a USB hold.
    pub fn usb_pair(&self) -> Option<UsbPair> {
        self.usb
    }

    /// The Web Lock this hold is taken under.
    pub fn lock_name(&self) -> String {
        match self.usb {
            Some(UsbPair { vendor, product }) => format!(
                "{LOCK_PREFIX}{USB_SEGMENT}:{vendor:04x}:{product:04x}:{}",
                self.mac
            ),
            None => format!("{LOCK_PREFIX}{NETWORK_SEGMENT}:{}", self.mac),
        }
    }

    /// The hold a Web Lock name names, or `None` for any other name — the
    /// library's locks, another app's, or a spelling that is not exactly the
    /// one [`Self::lock_name`] writes (a second spelling would be a second
    /// lock for the same board).
    pub fn parse(name: &str) -> Option<Self> {
        let rest = name.strip_prefix(LOCK_PREFIX)?;
        let parts: Vec<&str> = rest.split(':').collect();
        let key = match parts.as_slice() {
            [USB_SEGMENT, vendor, product, mac] => Self::usb(
                BoardKey::parse(mac).ok()?,
                UsbPair {
                    vendor: parse_id(vendor)?,
                    product: parse_id(product)?,
                },
            ),
            [NETWORK_SEGMENT, mac] => Self::network(BoardKey::parse(mac).ok()?),
            _ => return None,
        };
        (key.lock_name() == name).then_some(key)
    }
}

impl fmt::Display for HoldKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.lock_name())
    }
}

impl From<lpa_devices::link::UsbIds> for UsbPair {
    fn from(ids: lpa_devices::link::UsbIds) -> Self {
        Self {
            vendor: ids.vendor,
            product: ids.product,
        }
    }
}

/// On the hold channel a key travels as its lock name: one spelling, the
/// same one the lock manager lists.
impl Serialize for HoldKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.lock_name())
    }
}

impl<'de> Deserialize<'de> for HoldKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::parse(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("not a board hold: {name:?}")))
    }
}

/// Whether `info`'s link reaches its board through the board's one network
/// slot: the LAN (`lan:`) or lightplayer.app's relay (`relay:`).
pub fn is_network_road(info: &LinkInfo) -> bool {
    info.endpoint.is_lan() || info.endpoint.is_relay()
}

/// Four lowercase hex digits (one USB id, as the lock name writes it).
fn parse_id(text: &str) -> Option<u16> {
    if text.len() != 4 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(text, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_names_are_the_documented_scheme() {
        assert_eq!(usb_key().lock_name(), "lp-board:usb:303a:1001:a0f26287b48c");
        assert_eq!(
            HoldKey::network(mac()).lock_name(),
            "lp-board:net:a0f26287b48c"
        );
        assert_eq!(
            HoldKey::usb(
                mac(),
                UsbPair {
                    vendor: 0x1a86,
                    product: 0x7523
                }
            )
            .lock_name(),
            "lp-board:usb:1a86:7523:a0f26287b48c"
        );
    }

    #[test]
    fn lock_names_parse_back_to_their_keys() {
        for key in [usb_key(), HoldKey::network(mac())] {
            assert_eq!(HoldKey::parse(&key.lock_name()), Some(key));
        }
    }

    #[test]
    fn other_names_and_other_spellings_are_not_board_holds() {
        for name in [
            "lp-project:prjabc123",
            "lp-catalog",
            "some-other-lock",
            "",
            "lp-board:",
            "lp-board:usb:303a:1001",
            "lp-board:usb:303a:1001:a0f26287b48c:extra",
            "lp-board:net:a0f26287b48c:extra",
            "lp-board:wifi:a0f26287b48c",
            // Not the one spelling: upper case, separators, short ids.
            "lp-board:usb:303A:1001:a0f26287b48c",
            "lp-board:usb:303a:1001:A0F26287B48C",
            "lp-board:net:a0:f2:62:87:b4:8c",
            "lp-board:usb:303a:101:a0f26287b48c",
            // Not a board: a failed efuse read.
            "lp-board:net:000000000000",
            "lp-board:net:ffffffffffff",
        ] {
            assert_eq!(HoldKey::parse(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_key_travels_as_its_lock_name() {
        let json = serde_json::to_string(&usb_key()).expect("serialize");
        assert_eq!(json, "\"lp-board:usb:303a:1001:a0f26287b48c\"");
        let back: HoldKey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, usb_key());
        assert!(serde_json::from_str::<HoldKey>("\"lp-catalog\"").is_err());
    }

    #[test]
    fn a_link_is_held_by_its_port_or_by_the_boards_network_slot() {
        let usb = LinkInfo {
            label: "/dev/cu.usbmodem2101".to_string(),
            endpoint: lpa_devices::EndpointKey("browser-serial-esp32-port-1".to_string()),
            usb: Some(lpa_devices::link::UsbIds {
                vendor: 0x303a,
                product: 0x1001,
            }),
            serial_number: None,
            carries_update_channel: true,
        };
        assert_eq!(HoldKey::of_link(mac(), &usb), Some(usb_key()));
        let lan = lpa_link::providers::network_link::lan_link_info("ws://10.0.0.5/link");
        assert_eq!(HoldKey::of_link(mac(), &lan), Some(HoldKey::network(mac())));
        let relay = lpa_link::providers::network_link::relay_link_info(
            &lpa_link::providers::network_link::relay_socket_url(
                "https://lightplayer.app",
                "a0f26287b48c",
            ),
        )
        .expect("a relay leg");
        assert_eq!(
            HoldKey::of_link(mac(), &relay),
            Some(HoldKey::network(mac())),
            "the LAN and the relay are one slot"
        );
        let ble = LinkInfo {
            endpoint: lpa_devices::EndpointKey("ble:abc".to_string()),
            ..lan
        };
        assert_eq!(HoldKey::of_link(mac(), &ble), None);
    }

    #[test]
    fn the_key_names_its_board_the_way_the_model_does() {
        assert_eq!(
            usb_key().mac_address(),
            MacAddress("a0:f2:62:87:b4:8c".to_string())
        );
        assert_eq!(usb_key().via(), HoldVia::Usb);
        assert_eq!(HoldKey::network(mac()).usb_pair(), None);
    }

    fn mac() -> BoardKey {
        BoardKey::parse("a0:f2:62:87:b4:8c").expect("a mac")
    }

    fn usb_key() -> HoldKey {
        HoldKey::usb(
            mac(),
            UsbPair {
                vendor: 0x303a,
                product: 0x1001,
            },
        )
    }
}

//! The board's first frame on the device leg.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_access::SALT_BYTES;

use crate::lan_address::LanAddress;
use crate::relay_limits::{MAX_FIRMWARE_BYTES, MAX_HELLO_ACCOUNTS, MAX_LABEL_BYTES};
use crate::relay_version::{RELAY_PROTO_1, RELAY_PROTO_2};

/// Who the board is and which accounts it says it belongs to.
///
/// The hub answers it with a [`RelayFrame::Challenge`](crate::RelayFrame::Challenge),
/// and the board proves each account with one proof, in `accounts`' order.
///
/// A protocol 1 hello is [`Self::new`]; a protocol 2 hello is
/// `RelayHello::new(…).with_firmware(…)`, which adds the firmware version
/// as the hello's tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayHello {
    /// The device-leg protocol the board speaks. First on the wire, so the
    /// hub can refuse a version before decoding the rest.
    pub relay_proto: u16,
    /// The board's MAC: its name at the hub and in `/relay/board/<mac>`.
    pub board_mac: [u8; 6],
    /// The board's name for people, at most [`MAX_LABEL_BYTES`] of UTF-8.
    pub label: String,
    /// The device wire version the board speaks (`WIRE_PROTO_VERSION`), so
    /// a browser can tell before it connects whether it can talk to it.
    pub wire_proto: u32,
    /// Where the board answers on its own network, when it has an address.
    pub lan: Option<LanAddress>,
    /// The salts of the board's account-key entries, at most
    /// [`MAX_HELLO_ACCOUNTS`].
    pub accounts: Vec<[u8; SALT_BYTES]>,
    /// The version of the firmware the board runs (on a split C6, the
    /// core's), at most [`MAX_FIRMWARE_BYTES`] of ASCII. `None` on a
    /// protocol 1 hello, which has no such field; `Some` on protocol 2,
    /// where an empty string means "unknown".
    pub firmware: Option<String>,
}

impl RelayHello {
    /// **The protocol 1 hello**: the fields every version carries, at
    /// [`RELAY_PROTO_1`], with no firmware. Its label is cut to
    /// [`MAX_LABEL_BYTES`] (on a character boundary) and its accounts to the
    /// first [`MAX_HELLO_ACCOUNTS`]. [`Self::with_firmware`] makes it a
    /// protocol 2 hello.
    ///
    /// It stays protocol 1 whatever this build's
    /// [`RELAY_PROTO_VERSION`](crate::RELAY_PROTO_VERSION) is: protocol 1's
    /// golden bytes (`tests/relay_frame_golden.rs`) are built with it.
    #[must_use]
    pub fn new(
        board_mac: [u8; 6],
        label: &str,
        wire_proto: u32,
        lan: Option<LanAddress>,
        mut accounts: Vec<[u8; SALT_BYTES]>,
    ) -> Self {
        accounts.truncate(MAX_HELLO_ACCOUNTS);
        Self {
            relay_proto: RELAY_PROTO_1,
            board_mac,
            label: String::from(cut_label(label)),
            wire_proto,
            lan,
            accounts,
            firmware: None,
        }
    }

    /// This hello at [`RELAY_PROTO_2`], carrying `firmware` as the board's
    /// firmware version: cut to [`MAX_FIRMWARE_BYTES`] on a character
    /// boundary, with any character that is not ASCII written as `?` (the
    /// field is ASCII on the wire, so a hello this builds always decodes).
    #[must_use]
    pub fn with_firmware(mut self, firmware: &str) -> Self {
        self.relay_proto = RELAY_PROTO_2;
        self.firmware = Some(firmware_field(firmware));
        self
    }
}

/// The longest prefix of `label` that fits [`MAX_LABEL_BYTES`] and ends on
/// a character boundary.
pub(crate) fn cut_label(label: &str) -> &str {
    cut_utf8(label, MAX_LABEL_BYTES)
}

/// The longest prefix of `text` that fits `max` bytes and ends on a
/// character boundary.
pub(crate) fn cut_utf8(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `firmware` as the hello's firmware field: cut to [`MAX_FIRMWARE_BYTES`]
/// on a character boundary, every character that is not ASCII written as
/// `?`.
pub(crate) fn firmware_field(firmware: &str) -> String {
    cut_utf8(firmware, MAX_FIRMWARE_BYTES)
        .chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn new_cuts_the_label_on_a_character_boundary_and_caps_the_accounts() {
        // 31 ASCII bytes then a two-byte character straddling the limit.
        let label = "abcdefghijklmnopqrstuvwxyz01234é";
        let hello = RelayHello::new([1; 6], label, 39, None, vec![[7; 16]; 12]);
        assert_eq!(hello.label, "abcdefghijklmnopqrstuvwxyz01234");
        assert_eq!(hello.accounts.len(), MAX_HELLO_ACCOUNTS);
        assert_eq!(hello.relay_proto, RELAY_PROTO_1);
    }

    #[test]
    fn new_is_protocol_1_with_no_firmware() {
        let hello = RelayHello::new([1; 6], "Lamp", 39, None, vec![]);
        assert_eq!(hello.relay_proto, RELAY_PROTO_1);
        assert_eq!(hello.firmware, None);
    }

    #[test]
    fn with_firmware_is_protocol_2() {
        let hello = RelayHello::new([1; 6], "Lamp", 39, None, vec![]).with_firmware("2026.10.09-1");
        assert_eq!(hello.relay_proto, RELAY_PROTO_2);
        assert_eq!(hello.firmware.as_deref(), Some("2026.10.09-1"));
        let unknown = RelayHello::new([1; 6], "Lamp", 39, None, vec![]).with_firmware("");
        assert_eq!(unknown.relay_proto, RELAY_PROTO_2);
        assert_eq!(unknown.firmware.as_deref(), Some(""));
    }

    #[test]
    fn with_firmware_cuts_on_a_character_boundary_and_stays_ascii() {
        // 40 ASCII bytes, then more.
        let long = "0123456789012345678901234567890123456789-and-more";
        let hello = RelayHello::new([1; 6], "", 1, None, vec![]).with_firmware(long);
        let firmware = hello.firmware.unwrap();
        assert_eq!(firmware.len(), MAX_FIRMWARE_BYTES);
        assert_eq!(firmware, &long[..40]);

        // 39 ASCII bytes then a two-byte character straddling the limit.
        let cut = RelayHello::new([1; 6], "", 1, None, vec![])
            .with_firmware("012345678901234567890123456789012345678é");
        assert_eq!(
            cut.firmware.as_deref(),
            Some("012345678901234567890123456789012345678"),
            "the two-byte character straddles byte 40, so it goes"
        );

        let accented = RelayHello::new([1; 6], "", 1, None, vec![]).with_firmware("v1-é");
        assert_eq!(accented.firmware.as_deref(), Some("v1-?"));
    }
}

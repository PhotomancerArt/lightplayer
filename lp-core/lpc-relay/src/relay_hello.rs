//! The board's first frame on the device leg.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_access::SALT_BYTES;

use crate::lan_address::LanAddress;
use crate::relay_limits::{MAX_HELLO_ACCOUNTS, MAX_LABEL_BYTES};
use crate::relay_version::RELAY_PROTO_VERSION;

/// Who the board is and which accounts it says it belongs to.
///
/// The hub answers it with a [`RelayFrame::Challenge`](crate::RelayFrame::Challenge),
/// and the board proves each account with one proof, in `accounts`' order.
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
}

impl RelayHello {
    /// A hello at this build's [`RELAY_PROTO_VERSION`], its label cut to
    /// [`MAX_LABEL_BYTES`] (on a character boundary) and its accounts to the
    /// first [`MAX_HELLO_ACCOUNTS`].
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
            relay_proto: RELAY_PROTO_VERSION,
            board_mac,
            label: String::from(cut_label(label)),
            wire_proto,
            lan,
            accounts,
        }
    }
}

/// The longest prefix of `label` that fits [`MAX_LABEL_BYTES`] and ends on
/// a character boundary.
pub(crate) fn cut_label(label: &str) -> &str {
    if label.len() <= MAX_LABEL_BYTES {
        return label;
    }
    let mut end = MAX_LABEL_BYTES;
    while !label.is_char_boundary(end) {
        end -= 1;
    }
    &label[..end]
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
        assert_eq!(hello.relay_proto, RELAY_PROTO_VERSION);
    }
}

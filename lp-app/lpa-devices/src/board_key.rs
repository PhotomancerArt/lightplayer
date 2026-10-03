//! [`BoardKey`]: the one id a board goes by everywhere Studio names it —
//! its MAC, as 12 lowercase hex digits (`a0f26287b48c`).
//!
//! Real boards use their silicon (efuse) base MAC. Sims and emulated boards
//! are minted one with [`BoardKey::locally_administered`]: the IEEE's
//! locally administered range, which no vendor's hardware ever ships with,
//! so a made board can never take a real board's id.
//!
//! A key is NOT persisted in this form. What the records hold is the
//! [`MacAddress`] text the hello reports (`a0:f2:62:87:b4:8c`); a key is
//! read off it ([`BoardKey::from_mac`]) wherever an id is needed, so no
//! stored byte changes shape.

use core::fmt;
use core::str::FromStr;

use crate::identity::MacAddress;

/// One board's id: its six MAC octets. Displayed (and parsed) as 12
/// lowercase hex digits with no separators.
///
/// Parsing accepts every spelling the code already holds — colons
/// (`a0:f2:62:87:b4:8c`, the hello's), dashes, upper case, and the bare
/// canonical form — and refuses the all-zero and all-ones addresses, which
/// are what a *failed* efuse read looks like: every board whose read failed
/// would otherwise answer to one id.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoardKey([u8; 6]);

impl BoardKey {
    /// A key from its six octets, or `None` for an address that cannot be
    /// an identity (all zero, all ones).
    pub fn from_octets(octets: [u8; 6]) -> Option<Self> {
        (octets != [0x00; 6] && octets != [0xff; 6]).then_some(Self(octets))
    }

    /// A made board's key, minted from six caller-supplied random bytes
    /// (sans-IO: the caller owns entropy).
    ///
    /// Octet 0 becomes `(random & 0xFC) | 0x02`: the locally administered
    /// bit set and the multicast bit cleared. That is the range the IEEE
    /// reserves for addresses nobody bought, and it also puts the two
    /// refused addresses out of reach (all-zero needs bit 1 clear, all-ones
    /// needs bit 0 set), so this cannot fail.
    pub fn locally_administered(random: [u8; 6]) -> Self {
        let mut octets = random;
        octets[0] = (octets[0] & 0xfc) | 0x02;
        Self(octets)
    }

    /// The key a reported MAC names, or `None` when the text is not one.
    pub fn from_mac(mac: &MacAddress) -> Option<Self> {
        Self::parse(&mac.0).ok()
    }

    /// Read any spelling of a MAC: `a0:f2:62:87:b4:8c`, `A0-F2-62-87-B4-8C`,
    /// `a0f26287b48c`.
    pub fn parse(text: &str) -> Result<Self, BoardKeyError> {
        let digits: Vec<u8> = text
            .trim()
            .bytes()
            .filter(|byte| *byte != b':' && *byte != b'-')
            .collect();
        if digits.len() != 12 || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(BoardKeyError::NotAMac(text.to_string()));
        }
        let mut octets = [0u8; 6];
        for (octet, pair) in octets.iter_mut().zip(digits.chunks(2)) {
            *octet = (hex_value(pair[0]) << 4) | hex_value(pair[1]);
        }
        Self::from_octets(octets).ok_or_else(|| BoardKeyError::NotAnIdentity(text.to_string()))
    }

    /// The six octets.
    pub fn octets(&self) -> [u8; 6] {
        self.0
    }

    /// Whether this is a made board's key (the locally administered bit).
    pub fn is_locally_administered(&self) -> bool {
        self.0[0] & 0x02 == 0x02
    }

    /// The MAC text the records and the hello use: `a0:f2:62:87:b4:8c`.
    pub fn to_mac_address(&self) -> MacAddress {
        let [a, b, c, d, e, f] = self.0;
        MacAddress(format!("{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{f:02x}"))
    }
}

impl fmt::Display for BoardKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for octet in self.0 {
            write!(f, "{octet:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for BoardKey {
    type Err = BoardKeyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Why a text could not be read as a [`BoardKey`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoardKeyError {
    /// Not six hex octets.
    NotAMac(String),
    /// Six octets, but the all-zero or all-ones address a failed efuse read
    /// produces.
    NotAnIdentity(String),
}

impl fmt::Display for BoardKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAMac(text) => write!(f, "`{text}` is not a MAC address (six hex octets)"),
            Self::NotAnIdentity(text) => write!(
                f,
                "`{text}` is what a failed efuse read looks like, not a board's MAC"
            ),
        }
    }
}

impl std::error::Error for BoardKeyError {}

fn hex_value(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => digit - b'A' + 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_reads_as_one_canonical_key() {
        let canonical = "a0f26287b48c";
        for text in [
            "a0:f2:62:87:b4:8c",
            "A0:F2:62:87:B4:8C",
            "a0-f2-62-87-b4-8c",
            "A0F26287B48C",
            "a0f26287b48c",
            " a0:f2:62:87:b4:8c ",
        ] {
            let key = BoardKey::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"));
            assert_eq!(key.to_string(), canonical, "{text}");
        }
        assert_eq!(
            BoardKey::from_mac(&MacAddress("a0:f2:62:87:b4:8c".to_string()))
                .map(|key| key.to_mac_address()),
            Some(MacAddress("a0:f2:62:87:b4:8c".to_string())),
            "the records' own spelling round-trips"
        );
        assert_eq!(
            canonical.parse::<BoardKey>().unwrap().to_string(),
            canonical
        );
    }

    #[test]
    fn what_is_not_a_board_mac_is_refused() {
        for text in ["", "a0:f2:62", "a0:f2:62:87:b4:8c:00:11", "g0f26287b48c"] {
            assert!(
                matches!(BoardKey::parse(text), Err(BoardKeyError::NotAMac(_))),
                "{text}"
            );
        }
        for text in ["00:00:00:00:00:00", "ff:ff:ff:ff:ff:ff"] {
            assert!(
                matches!(BoardKey::parse(text), Err(BoardKeyError::NotAnIdentity(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn a_minted_key_is_locally_administered_unicast_whatever_the_bytes() {
        let key = BoardKey::locally_administered([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(key.to_string(), "122233445566", "(0x11 & 0xfc) | 0x02");
        assert!(key.is_locally_administered());

        for random in [[0x00; 6], [0xff; 6], [0x01; 6]] {
            let key = BoardKey::locally_administered(random);
            assert_eq!(key.octets()[0] & 0x03, 0x02, "{key}");
            assert_eq!(
                BoardKey::from_octets(key.octets()),
                Some(key),
                "never a refused address"
            );
        }
        assert!(
            !BoardKey::parse("a0:f2:62:87:b4:8c")
                .unwrap()
                .is_locally_administered(),
            "silicon's MAC is a bought one"
        );
    }
}

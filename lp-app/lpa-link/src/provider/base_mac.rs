//! Normalizing a *reported* base MAC into the one spelling identity uses.
//!
//! The chip's factory base MAC is the only globally unique, erase-proof fact
//! a board carries, and two reporters name it two ways:
//!
//! | reporter | spelling |
//! |---|---|
//! | firmware hello (`HardwareFacts::base_mac`) | `60:55:f9:01:02:03` |
//! | esptool-js `chip.readMac(loader)` | `60:55:f9:01:02:03` |
//!
//! They already agree today — both build the string from `%02x` octets — but
//! the download-mode read crosses an untyped JS boundary, so "agreeing" is a
//! thing to CHECK rather than assume. [`normalize_base_mac`] is that check:
//! lowercase colon-hex out, `None` for anything that is not exactly six hex
//! octets.
//!
//! The width matters more than it looks. A C6 also carries an 802.15.4
//! EUI-64 (`HardwareFacts::eui64`) — eight octets in the same colon-hex
//! spelling — and quietly accepting one as a MAC would mint a second,
//! different identity for the same board.

/// The chip's base MAC in canonical form (lowercase colon hex), or `None`
/// when `reported` is not one.
///
/// `None` is an answer, not an error: A2 evidence is optional by design and
/// a board that will not name itself is simply anonymous for this session.
/// What it must never do is invent an identity, so the rejections are
/// deliberate:
///
/// - the wrong number of octets — an EUI-64 (8) most of all;
/// - anything that is not two hex digits per octet;
/// - the all-zero and all-ones addresses, which are what a *failed* efuse
///   read looks like (0x00000000 / 0xffffffff registers). Those two are the
///   dangerous garbage: unlike a malformed string they parse fine, and every
///   board whose read failed would answer to the same identity.
pub fn normalize_base_mac(reported: &str) -> Option<String> {
    let mut octets = [0u8; 6];
    let mut count = 0;
    for part in reported.trim().split(':') {
        if part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        // `count` past the end is the too-many-octets rejection (an EUI-64
        // arrives here), so index fallibly rather than pushing.
        *octets.get_mut(count)? = u8::from_str_radix(part, 16).ok()?;
        count += 1;
    }
    if count != octets.len() {
        return None;
    }
    if octets.iter().all(|octet| *octet == 0x00) || octets.iter().all(|octet| *octet == 0xff) {
        return None;
    }
    Some(
        octets
            .iter()
            .map(|octet| format!("{octet:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two spellings that actually reach this function: the hello's
    /// (already canonical) and esptool-js's download-mode read. Uppercase
    /// and mixed case are accepted because a MAC is the same address however
    /// it is written down — only the STORED form is canonical.
    #[test]
    fn every_reporter_spelling_normalizes_to_lowercase_colon_hex() {
        for reported in [
            "60:55:f9:0a:0b:0c",
            "60:55:F9:0A:0B:0C",
            "60:55:F9:0a:0B:0c",
            "60:55:f9:0a:0b:0c\n",
            " 60:55:F9:0A:0B:0C ",
        ] {
            assert_eq!(
                normalize_base_mac(reported).as_deref(),
                Some("60:55:f9:0a:0b:0c"),
                "{reported}"
            );
        }
    }

    /// A leading zero octet must survive as `00`, not collapse to `0`: the
    /// derived uid embeds these bytes, so a lost digit is a different board.
    #[test]
    fn zero_octets_keep_both_digits() {
        assert_eq!(
            normalize_base_mac("00:0a:00:0b:00:0c").as_deref(),
            Some("00:0a:00:0b:00:0c")
        );
    }

    /// The width check, from both directions. The EUI-64 is the one that
    /// would otherwise slip through: it is the same chip, the same spelling,
    /// and a different address.
    #[test]
    fn only_six_octets_are_a_base_mac() {
        assert_eq!(normalize_base_mac("60:55:f9:01:02:03:04:05"), None);
        assert_eq!(normalize_base_mac("60:55:f9:01:02"), None);
        assert_eq!(normalize_base_mac("60:55:f9:01:02:03:04"), None);
    }

    #[test]
    fn malformed_octets_are_rejected() {
        for reported in [
            "",
            "60:55:f9:01:02:0",
            "60:55:f9:01:02:0g",
            "60-55-f9-01-02-03",
            "6055f9010203",
            "60:55:f9:01:02:003",
            "60:55:f9:01:02:03:",
            "MAC: 60:55:f9:01:02:03",
        ] {
            assert_eq!(normalize_base_mac(reported), None, "{reported}");
        }
    }

    /// What a failed efuse read produces. Storing either would hand every
    /// board that failed the same identity — worse than having none.
    #[test]
    fn the_failed_read_addresses_are_not_identities() {
        assert_eq!(normalize_base_mac("00:00:00:00:00:00"), None);
        assert_eq!(normalize_base_mac("ff:ff:ff:ff:ff:ff"), None);
        assert_eq!(normalize_base_mac("FF:FF:FF:FF:FF:FF"), None);
        // A single non-zero octet makes it a plausible address again.
        assert_eq!(
            normalize_base_mac("00:00:00:00:00:01").as_deref(),
            Some("00:00:00:00:00:01")
        );
    }

    /// Idempotence: normalizing evidence that has already been normalized
    /// (a snapshot round-tripping through a consumer) must not change it.
    #[test]
    fn normalizing_a_normalized_mac_is_a_no_op() {
        let once = normalize_base_mac("60:55:F9:01:02:03").unwrap();
        assert_eq!(normalize_base_mac(&once), Some(once.clone()));
    }
}

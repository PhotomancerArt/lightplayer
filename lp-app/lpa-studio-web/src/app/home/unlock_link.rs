//! The share link (plan D14): `https://<origin>/unlock#<device>&<password>`.
//!
//! The password rides in the `#fragment`, which a browser never sends to a
//! server — opening the link reaches our page and nothing else. The page
//! saves the password in that browser's remembered passwords and offers
//! Connect; the fragment is cleared from the address bar as soon as it is
//! read.
//!
//! Both parts are percent-encoded the way `encodeURIComponent` does it
//! (everything but `A–Z a–z 0–9 - _ . ! ~ * ' ( )`), so `&` and `#` in a
//! device name or a typed password survive the trip.

/// The path the link opens.
pub(crate) const UNLOCK_PATH: &str = "/unlock";

/// What a share link carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnlockLink {
    pub device_name: String,
    pub password: String,
}

impl UnlockLink {
    /// The full link, from `origin` (`https://lightplayer.app`).
    pub(crate) fn url(&self, origin: &str) -> String {
        format!(
            "{}{UNLOCK_PATH}#{}&{}",
            origin.trim_end_matches('/'),
            encode_component(&self.device_name),
            encode_component(&self.password)
        )
    }

    /// Read a fragment (with or without its `#`). `None` when there is no
    /// password in it.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(
            dead_code,
            reason = "read by the wasm boot capture; host builds only run the unit tests"
        )
    )]
    pub(crate) fn from_fragment(fragment: &str) -> Option<Self> {
        let fragment = fragment.strip_prefix('#').unwrap_or(fragment);
        let (device, password) = fragment.split_once('&')?;
        let password = decode_component(password)?;
        if password.is_empty() {
            return None;
        }
        Some(Self {
            device_name: decode_component(device).unwrap_or_default(),
            password,
        })
    }
}

/// `encodeURIComponent`.
pub(crate) fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            );
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `decodeURIComponent`: `None` on a broken escape or bytes that are not
/// UTF-8.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm boot capture; host builds only run the unit tests"
    )
)]
pub(crate) fn decode_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_carries_the_device_and_the_password_in_the_fragment() {
        let link = UnlockLink {
            device_name: "PLAYFUL choker".to_string(),
            password: "maple-otter-42".to_string(),
        };
        assert_eq!(
            link.url("https://lightplayer.app"),
            "https://lightplayer.app/unlock#PLAYFUL%20choker&maple-otter-42"
        );
    }

    #[test]
    fn anything_typed_survives_the_round_trip() {
        let link = UnlockLink {
            device_name: "Mo's & Jo's #1 dome".to_string(),
            password: "s'mores & 100% ☀".to_string(),
        };
        let url = link.url("https://lightplayer.app/");
        let fragment = url.split_once('#').unwrap().1;
        assert_eq!(fragment.matches('&').count(), 1, "{fragment}");
        assert_eq!(UnlockLink::from_fragment(fragment), Some(link));
    }

    #[test]
    fn a_fragment_with_no_password_is_not_a_link() {
        assert_eq!(UnlockLink::from_fragment(""), None);
        assert_eq!(UnlockLink::from_fragment("#choker"), None);
        assert_eq!(UnlockLink::from_fragment("#choker&"), None);
        assert_eq!(UnlockLink::from_fragment("#choker&%zz"), None);
        assert_eq!(
            UnlockLink::from_fragment("#&camp"),
            Some(UnlockLink {
                device_name: String::new(),
                password: "camp".to_string()
            })
        );
    }
}

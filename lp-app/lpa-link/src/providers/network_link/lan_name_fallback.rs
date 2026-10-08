//! Where else to look for a board on the LAN when its address stops
//! answering: its own mDNS name, `lp-xxxx.local` (OTA Wi‑Fi plan P7).
//!
//! A board's IP is the router's to give, and it can change across a reset —
//! an over-the-air update resets the board three times. A session dialled
//! at an IP keeps redialling that IP; once the board's hello has said its
//! base MAC, the page also tries the name the board answers on the LAN (the
//! firmware's `mdns_name`: `lp-` and the MAC's last two bytes in hex). Only
//! for a dotted IPv4 that is not loopback: a `.local` address is already the
//! name, and `127.0.0.1` is an emulator's port forward, not the board.

/// The board's `.local` socket URL to try beside `url`, the socket it was
/// dialled at, once its hello said `base_mac` (any case, any separator:
/// `A0:F2:62:87:B4:8C`). `None` when `url` is not a LAN IPv4 address or the
/// MAC does not read.
pub fn lan_name_fallback(url: &str, base_mac: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let (authority, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, "/link"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    };
    let octets: Vec<u8> = host
        .split('.')
        .map(|part| part.parse::<u8>().ok())
        .collect::<Option<_>>()?;
    if octets.len() != 4 || octets[0] == 127 {
        return None;
    }
    let hex: String = base_mac
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_ascii_lowercase();
    if hex.len() != 12 {
        return None;
    }
    let port = port.map(|port| format!(":{port}")).unwrap_or_default();
    Some(format!("{scheme}://lp-{}.local{port}{path}", &hex[8..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ip_address_falls_back_to_the_boards_name() {
        assert_eq!(
            lan_name_fallback("ws://192.168.1.40/link", "A0:F2:62:87:B4:8C").as_deref(),
            Some("ws://lp-b48c.local/link")
        );
        assert_eq!(
            lan_name_fallback("ws://10.0.0.9:8080/link", "10bda3b08e30").as_deref(),
            Some("ws://lp-8e30.local:8080/link")
        );
    }

    #[test]
    fn a_name_a_forward_or_an_unreadable_mac_has_none() {
        let mac = "a0:f2:62:87:b4:8c";
        assert_eq!(lan_name_fallback("ws://lp-b48c.local/link", mac), None);
        assert_eq!(lan_name_fallback("ws://127.0.0.1:5591/link", mac), None);
        assert_eq!(lan_name_fallback("ws://192.168.1.40/link", "a0:f2"), None);
        assert_eq!(lan_name_fallback("192.168.1.40", mac), None);
    }
}

//! The boards a `?lan=` dev flag names (Wi-Fi M6 P07).
//!
//! `?lan=<url>[,<url>…]`, read once at page load like `?emu=`: each value is
//! a board's link socket, and each becomes a network device on the Devices
//! page. There is no UI to add one by hand (roadmap M8); this is how a desk
//! or an emulated board on the LAN reaches Studio until then.
//!
//! Each value is normalised to the URL Studio dials:
//!
//! | written | dialled |
//! |---|---|
//! | `ws://192.168.1.40/link` | as written |
//! | `ws://192.168.1.40` | `ws://192.168.1.40/link` |
//! | `192.168.1.40`, `lp-b48c.local:8080` | `ws://…/link` |
//! | `wss://…` | as written (a TLS proxy in front of a board) |
//!
//! Anything else — another scheme, credentials, an empty host — is refused
//! by name, and the page says so once in the console. The query value may be
//! percent-encoded (`ws%3A%2F%2F…`), as a browser writes one.

/// The path a board serves its link on.
pub const LAN_LINK_PATH: &str = "/link";

/// What a `?lan=` value named.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LanFlag {
    /// The sockets to dial, normalised, in order, without repeats.
    pub addresses: Vec<String>,
    /// Values that were not a board's address, each with why.
    pub refused: Vec<(String, String)>,
}

/// Parse a `?lan=` value (still percent-encoded, as it sits in the query).
pub fn parse_lan_flag(raw: &str) -> LanFlag {
    let mut flag = LanFlag::default();
    for part in percent_decode(raw).split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match normalize_lan_address(part) {
            Ok(url) => {
                if !flag.addresses.contains(&url) {
                    flag.addresses.push(url);
                }
            }
            Err(why) => flag.refused.push((part.to_string(), why)),
        }
    }
    flag
}

/// One value as the URL Studio dials, or why it is not a board's address.
pub fn normalize_lan_address(value: &str) -> Result<String, String> {
    let (scheme, rest) = match value.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("ws".to_string(), value),
    };
    if scheme != "ws" && scheme != "wss" {
        return Err(format!(
            "a board is reached at ws://<host>/link, not {scheme}://"
        ));
    }
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err("no host".to_string());
    }
    if authority.contains(['@', ' ']) {
        return Err("a board's address carries no credentials or spaces".to_string());
    }
    let path = if path.is_empty() || path == "/" {
        LAN_LINK_PATH
    } else {
        path
    };
    Ok(format!(
        "{scheme}://{}{path}",
        authority.to_ascii_lowercase()
    ))
}

/// `%XX` escapes decoded.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = value.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_link_url_is_dialled_as_written() {
        let flag = parse_lan_flag("ws://192.168.1.40/link");
        assert_eq!(flag.addresses, ["ws://192.168.1.40/link"]);
        assert!(flag.refused.is_empty());
    }

    #[test]
    fn a_bare_host_or_a_url_without_a_path_gets_the_link_path() {
        let flag = parse_lan_flag("192.168.1.40,ws://lp-b48c.local:8080,wss://desk.example/");
        assert_eq!(
            flag.addresses,
            [
                "ws://192.168.1.40/link",
                "ws://lp-b48c.local:8080/link",
                "wss://desk.example/link"
            ]
        );
    }

    #[test]
    fn several_boards_parse_in_order_without_repeats_and_percent_encoded() {
        let flag = parse_lan_flag("ws%3A%2F%2F10.0.0.5%2Flink%2C10.0.0.6, ws://10.0.0.5/link,");
        assert_eq!(flag.addresses, ["ws://10.0.0.5/link", "ws://10.0.0.6/link"]);
    }

    #[test]
    fn other_schemes_credentials_and_empty_hosts_are_refused_by_name() {
        let flag = parse_lan_flag("http://10.0.0.5/,ws://user@10.0.0.5/link,ws:///link");
        assert!(flag.addresses.is_empty());
        let refused: Vec<&str> = flag
            .refused
            .iter()
            .map(|(value, _)| value.as_str())
            .collect();
        assert_eq!(
            refused,
            ["http://10.0.0.5/", "ws://user@10.0.0.5/link", "ws:///link"]
        );
        assert!(flag.refused[0].1.contains("ws://"), "{:?}", flag.refused);
    }

    #[test]
    fn an_empty_flag_names_nothing() {
        assert_eq!(parse_lan_flag(""), LanFlag::default());
        assert_eq!(parse_lan_flag(" , "), LanFlag::default());
    }
}

//! The `?record=<url>` sink: which URLs the session recorder will stream
//! to, and the envelope it wraps each batch in. Pure (host-tested); the
//! browser half is `device_events_io.rs`.
//!
//! # Why the sink host is restricted
//!
//! A recording is the whole session, unredacted: every device frame, the
//! access handshakes, the project. The flag comes from the query string,
//! so a link someone else wrote could otherwise point it anywhere
//! (`?record=https://someone-else/…`) and the page would stream a user's
//! session to them. The recorder therefore only talks to THIS machine or
//! the local network — loopback (`localhost`, `127.0.0.0/8`, `[::1]`), the
//! RFC 1918 ranges (`10/8`, `172.16/12`, `192.168/16`) and mDNS
//! `*.local` names — and says so, visibly, when it refuses one.
//!
//! # Envelope
//!
//! Each page load is one recording session with a random id. Every POST
//! goes to the sink URL with `session=<id>` added to its query (the
//! receiver files each session separately); every line carries a
//! monotonic `seq` spliced in at the front; and the first line of a
//! session is a `session` record naming the build, the browser and the
//! page ([`session_line`]).

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm recorder; host builds only run the unit tests"
    )
)]

use std::net::Ipv4Addr;

/// The query parameter that turns the recorder on.
pub const RECORD_PARAM: &str = "record";

/// What the recorder decided about a `?record=` URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SinkCheck {
    /// Stream to it. `host` is the `host[:port]` the badge shows.
    Accepted { host: String },
    /// Refuse it, and say why (the badge and the console both do).
    Refused { host: String, reason: String },
}

/// Decide about a sink from its parsed URL parts — the browser's own
/// `URL` parser supplies them (`protocol` with its colon, `hostname`
/// without a port, `host` with one), so this judges exactly the host the
/// browser would connect to.
pub fn check_sink(protocol: &str, hostname: &str, host: &str) -> SinkCheck {
    if protocol != "http:" && protocol != "https:" {
        return SinkCheck::Refused {
            host: host.to_string(),
            reason: format!("{protocol} is not http or https"),
        };
    }
    if sink_host_is_local(hostname) {
        SinkCheck::Accepted {
            host: host.to_string(),
        }
    } else {
        SinkCheck::Refused {
            host: host.to_string(),
            reason: format!("{hostname} is not on this machine or local network"),
        }
    }
}

/// Whether `hostname` (as `URL.hostname` renders it: lowercase, IPv6 in
/// brackets) is loopback, RFC 1918 private, or an mDNS `.local` name.
pub fn sink_host_is_local(hostname: &str) -> bool {
    let hostname = hostname.to_ascii_lowercase();
    if hostname == "localhost" || hostname == "[::1]" || hostname == "::1" {
        return true;
    }
    if let Ok(ip) = hostname.parse::<Ipv4Addr>() {
        return ip.is_loopback() || ip.is_private();
    }
    // `a.local`, never the bare `local` or a name that merely contains it.
    hostname
        .strip_suffix(".local")
        .is_some_and(|name| !name.is_empty() && !name.ends_with('.'))
}

/// The raw (still percent-encoded) value of `?record=` in a
/// `location.search` string, when present and non-empty.
pub fn record_param(search: &str) -> Option<&str> {
    let query = search.strip_prefix('?').unwrap_or(search);
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == RECORD_PARAM)
        .map(|(_, value)| value)
        .filter(|value| !value.is_empty())
}

/// `url` with `session=<id>` added to its query.
pub fn with_session_param(url: &str, session: &str) -> String {
    let (base, fragment) = url.split_once('#').unwrap_or((url, ""));
    let separator = if base.contains('?') { '&' } else { '?' };
    let mut out = format!("{base}{separator}session={session}");
    if !fragment.is_empty() {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// A JSON-object line with `"seq":N` spliced in as its first field. A line
/// that is not an object (never produced here) passes through untouched.
pub fn with_seq(line: &str, seq: u64) -> String {
    match line.strip_prefix('{') {
        Some("}") => format!("{{\"seq\":{seq}}}"),
        Some(rest) => format!("{{\"seq\":{seq},{rest}"),
        None => line.to_string(),
    }
}

/// What the `session` line says about the page that recorded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionFacts {
    /// Seconds since the Unix epoch at page load.
    pub t: f64,
    /// The recording session id (also the POSTs' `session=`).
    pub recording: String,
    /// The deploy's `version.json`, when the page could fetch one.
    pub version: Option<String>,
    pub sha: Option<String>,
    pub channel: Option<String>,
    /// The git branch baked in at build time (dev builds).
    pub branch: Option<String>,
    pub user_agent: String,
    pub href: String,
}

/// The first line of a recording: `{"t":…,"kind":"session",…}`. Absent
/// facts are absent fields, not nulls (the device-event contract's rule).
pub fn session_line(facts: &SessionFacts) -> String {
    let mut object = serde_json::Map::new();
    object.insert("t".into(), serde_json::json!(facts.t));
    object.insert("kind".into(), "session".into());
    object.insert("recording".into(), facts.recording.clone().into());
    for (key, value) in [
        ("version", &facts.version),
        ("sha", &facts.sha),
        ("channel", &facts.channel),
        ("branch", &facts.branch),
    ] {
        if let Some(value) = value {
            object.insert(key.into(), value.clone().into());
        }
    }
    object.insert("user_agent".into(), facts.user_agent.clone().into());
    object.insert("href".into(), facts.href.clone().into());
    serde_json::Value::Object(object).to_string()
}

/// A recording session id from random bytes: 16 lowercase hex digits.
pub fn session_id(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts_are_local() {
        for host in ["localhost", "LOCALHOST", "127.0.0.1", "127.8.9.10", "[::1]"] {
            assert!(sink_host_is_local(host), "{host}");
        }
    }

    #[test]
    fn private_lan_ranges_are_local() {
        for host in [
            "10.0.0.5",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.20",
            "studio-mac.local",
        ] {
            assert!(sink_host_is_local(host), "{host}");
        }
    }

    #[test]
    fn public_and_lookalike_hosts_are_refused() {
        for host in [
            "example.com",
            "8.8.8.8",
            "172.15.0.1",
            "172.32.0.1",
            "192.169.0.1",
            "11.0.0.1",
            "local",
            ".local",
            "evil.local.example.com",
            "127.0.0.1.nip.io",
            "localhost.example.com",
            "[2001:db8::1]",
            "",
        ] {
            assert!(!sink_host_is_local(host), "{host}");
        }
    }

    #[test]
    fn check_sink_accepts_local_http_and_refuses_the_rest() {
        assert_eq!(
            check_sink("http:", "127.0.0.1", "127.0.0.1:4321"),
            SinkCheck::Accepted {
                host: "127.0.0.1:4321".to_string()
            }
        );
        assert_eq!(
            check_sink("https:", "attacker.example", "attacker.example"),
            SinkCheck::Refused {
                host: "attacker.example".to_string(),
                reason: "attacker.example is not on this machine or local network".to_string(),
            }
        );
        assert!(matches!(
            check_sink("ftp:", "127.0.0.1", "127.0.0.1"),
            SinkCheck::Refused { .. }
        ));
    }

    #[test]
    fn the_record_param_is_found_among_others() {
        assert_eq!(
            record_param("?emu=ws%3A%2F%2Fx&record=http%3A%2F%2F127.0.0.1%3A9%2Fingest"),
            Some("http%3A%2F%2F127.0.0.1%3A9%2Fingest")
        );
        assert_eq!(record_param("?recording=1"), None);
        assert_eq!(record_param("?record="), None);
        assert_eq!(record_param(""), None);
    }

    #[test]
    fn the_session_rides_the_query() {
        assert_eq!(
            with_session_param("http://127.0.0.1:9/ingest", "ab12"),
            "http://127.0.0.1:9/ingest?session=ab12"
        );
        assert_eq!(
            with_session_param("http://127.0.0.1:9/ingest?x=1#f", "ab12"),
            "http://127.0.0.1:9/ingest?x=1&session=ab12#f"
        );
    }

    #[test]
    fn seq_is_spliced_in_first() {
        assert_eq!(
            with_seq(r#"{"t":1.0,"kind":"route"}"#, 7),
            r#"{"seq":7,"t":1.0,"kind":"route"}"#
        );
        assert_eq!(with_seq("{}", 0), r#"{"seq":0}"#);
    }

    #[test]
    fn the_session_line_names_the_build_and_page() {
        let line = session_line(&SessionFacts {
            t: 10.0,
            recording: "ab12".to_string(),
            version: Some("v1.2.3".to_string()),
            sha: Some("abc".to_string()),
            channel: None,
            branch: None,
            user_agent: "UA".to_string(),
            href: "https://lightplayer.app/devices?record=x".to_string(),
        });
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["kind"], "session");
        assert_eq!(parsed["recording"], "ab12");
        assert_eq!(parsed["version"], "v1.2.3");
        assert_eq!(parsed["sha"], "abc");
        assert!(parsed.get("channel").is_none());
        assert_eq!(parsed["user_agent"], "UA");
        assert_eq!(parsed["t"], 10.0);
    }

    #[test]
    fn session_ids_are_hex() {
        assert_eq!(
            session_id(&[0, 1, 0xab, 0xff, 2, 3, 4, 5, 6]),
            "0001abff02030405"
        );
    }
}

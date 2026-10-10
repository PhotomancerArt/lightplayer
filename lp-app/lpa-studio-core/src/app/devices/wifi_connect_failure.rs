//! Why a connect over Wi‑Fi failed, in plain words.
//!
//! The page's socket says what happened in the browser's terms (`wi-fi
//! connect timed out after 10 s`, `… was closed (code 1006)`, a board's
//! close 1013 "try again later"); the person needs to know what to do. Four
//! answers cover it, decided here from the socket's words, the host and
//! whether the page is served over https:
//!
//! | what the socket said | the answer |
//! |---|---|
//! | the board closed it with 1013 | busy with another connection |
//! | `ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS` | Chrome blocked it |
//! | the connect timed out | couldn't reach the board at `<host>` |
//! | closed before it opened, a `.local` name | this browser couldn't find `<host>` |
//! | closed before it opened, an https page | Chrome blocked it (a public page reaching a private address waits for the Local Network grant) |
//! | closed before it opened, otherwise | couldn't reach the board at `<host>` |
//!
//! A refused address (another scheme, credentials) never reaches a socket:
//! [`WifiConnectFailure::NotAnAddress`] carries the reason.

/// What went wrong, as a person acts on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiConnectFailure {
    /// Chrome's Local Network check blocked the socket.
    Blocked,
    /// The board has another connection (its one LAN slot is taken).
    Busy,
    /// Nothing answered at the address.
    Unreachable { host: String },
    /// The browser could not resolve the `.local` name.
    NotFound { host: String },
    /// What was typed is not a board's address.
    NotAnAddress { reason: String },
}

/// The words for [`WifiConnectFailure::Blocked`].
pub const WIFI_BLOCKED_WORDS: &str = "Chrome blocked the connection to your local network. Allow it in the site's settings and try again.";
/// The words for [`WifiConnectFailure::Busy`].
pub const WIFI_BUSY_WORDS: &str = "Busy with another connection \u{2014} try again";

impl WifiConnectFailure {
    /// Read the socket's words (`raw`, as `browser_websocket.js` says them)
    /// for the board at `host`, on a page served over https or not.
    pub fn from_socket(raw: &str, host: &str, secure_page: bool) -> Self {
        let lower = raw.to_ascii_lowercase();
        if lower.contains("code 1013") {
            return Self::Busy;
        }
        if lower.contains("err_blocked_by_local_network_access_checks")
            || lower.contains("local network access")
        {
            return Self::Blocked;
        }
        let host = host.to_string();
        if lower.contains("timed out") {
            return Self::Unreachable { host };
        }
        if is_local_name(&host) {
            return Self::NotFound { host };
        }
        if secure_page && closed_before_open(&lower) {
            return Self::Blocked;
        }
        Self::Unreachable { host }
    }

    /// The sentence the card or Connect a board's Network row says.
    pub fn words(&self) -> String {
        match self {
            Self::Blocked => WIFI_BLOCKED_WORDS.to_string(),
            Self::Busy => WIFI_BUSY_WORDS.to_string(),
            Self::Unreachable { host } => {
                format!("Couldn't reach the board at {host}. Is it on this network?")
            }
            Self::NotFound { host } => format!(
                "This browser couldn't find {host}. Try its address instead (it's on the \
                 board's Wi\u{2011}Fi panel)."
            ),
            Self::NotAnAddress { reason } => format!("That isn't a board's address: {reason}."),
        }
    }
}

/// Whether `host` (with or without a port) is an mDNS name.
fn is_local_name(host: &str) -> bool {
    let name = match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => name,
        _ => host,
    };
    name.to_ascii_lowercase().ends_with(".local")
}

/// The socket never opened: it failed or was closed while connecting.
fn closed_before_open(lower: &str) -> bool {
    lower.contains("wi-fi connect") && (lower.contains("failed") || lower.contains("was closed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_board_says_try_again() {
        let failure = WifiConnectFailure::from_socket(
            "wi-fi link lost: the board closed the link (code 1013: try again later)",
            "10.0.0.5",
            true,
        );
        assert_eq!(failure, WifiConnectFailure::Busy);
        assert_eq!(
            failure.words(),
            "Busy with another connection \u{2014} try again"
        );
    }

    #[test]
    fn a_timeout_is_an_address_nothing_answered() {
        let failure = WifiConnectFailure::from_socket(
            "wi-fi connect timed out after 10 s",
            "192.168.1.40",
            true,
        );
        assert_eq!(
            failure.words(),
            "Couldn't reach the board at 192.168.1.40. Is it on this network?"
        );
    }

    #[test]
    fn closed_before_it_opened_reads_by_the_page_and_the_name() {
        let closed = "wi-fi connect to ws://10.0.0.5/link was closed (code 1006)";
        assert_eq!(
            WifiConnectFailure::from_socket(closed, "10.0.0.5", true),
            WifiConnectFailure::Blocked
        );
        assert_eq!(
            WifiConnectFailure::from_socket(closed, "10.0.0.5", false),
            WifiConnectFailure::Unreachable {
                host: "10.0.0.5".to_string()
            }
        );
        let local = WifiConnectFailure::from_socket(
            "wi-fi connect to ws://lp-1a2b.local/link failed",
            "lp-1a2b.local",
            true,
        );
        assert_eq!(
            local.words(),
            "This browser couldn't find lp-1a2b.local. Try its address instead (it's on the \
             board's Wi\u{2011}Fi panel)."
        );
        assert!(matches!(
            WifiConnectFailure::from_socket("wi-fi connect failed", "lp-1a2b.local:8080", false),
            WifiConnectFailure::NotFound { .. }
        ));
    }

    #[test]
    fn chromes_own_block_is_named_even_on_plain_http() {
        let failure = WifiConnectFailure::from_socket(
            "net::ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS",
            "10.0.0.5",
            false,
        );
        assert_eq!(failure.words(), WIFI_BLOCKED_WORDS);
    }

    #[test]
    fn a_refused_address_says_why() {
        let failure = WifiConnectFailure::NotAnAddress {
            reason: "a board is reached at ws://<host>/link, not http://".to_string(),
        };
        assert_eq!(
            failure.words(),
            "That isn't a board's address: a board is reached at ws://<host>/link, not http://."
        );
    }
}

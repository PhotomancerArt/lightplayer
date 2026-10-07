//! Why a connect through lightplayer.app's relay failed, in plain words.
//!
//! The relay refuses after the upgrade, as a close code (a browser cannot
//! read a refused upgrade's status): `browser_websocket.js` says it as
//! `relay link lost: the board closed the link (code 4404: board-offline)`.
//! The link above the socket gives a session up when no key this browser
//! holds opens the board ([`RELAY_NO_HELD_KEY`]). The person needs to know
//! what to do:
//!
//! | what happened | the answer |
//! |---|---|
//! | no held key opens the board (or there is none) | sign in, and plug the board in once |
//! | `4404` board offline | the board isn't online |
//! | `4429` busy (the board's one network slot is taken) | busy with another connection |
//! | `4401` no signed-in session | sign in to lightplayer.app |
//! | `4420` too many tries from this network | wait a minute |
//! | `4410` the board went away, `1001` the relay restarting | `lpc_relay`'s own words |
//! | the socket never opened, or timed out | couldn't reach lightplayer.app |
//!
//! Every relay close code's number and its words live in
//! [`lpc_relay::RelayCloseCode`], shared with lp-cli; this file picks the
//! ones Studio says differently (the network-transport plan's P05) and keeps
//! the rest.
//!
//! [`RELAY_NO_HELD_KEY`]: lpa_link::providers::browser_websocket::RELAY_NO_HELD_KEY

use lpc_relay::RelayCloseCode;

use super::wifi_connect_failure::WIFI_BUSY_WORDS;

/// Why the relay session was given up when no held key opens the board —
/// `lpa-link`'s `RELAY_NO_HELD_KEY`, spelled here so this file is
/// host-tested without the wasm provider (a test pins the two equal).
pub const RELAY_NO_HELD_KEY_PHRASE: &str = "no key this browser holds opens this board";

/// The words for [`RelayConnectFailure::NoHeldKey`].
pub const RELAY_NO_HELD_KEY_WORDS: &str =
    "Sign in to Studio and plug this board in once to reach it through lightplayer.app.";
/// The words for [`RelayConnectFailure::Offline`].
pub const RELAY_OFFLINE_WORDS: &str = "The board isn't online.";
/// The words for [`RelayConnectFailure::Unreachable`].
pub const RELAY_UNREACHABLE_WORDS: &str = "Couldn't reach lightplayer.app. Are you online?";

/// What went wrong, as a person acts on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayConnectFailure {
    /// No key this browser holds opens the board: not signed in to the
    /// account the board knows, or the board never met it.
    NoHeldKey,
    /// `4404`: the board is not connected to lightplayer.app.
    Offline,
    /// `4429`: the board's one network connection is taken.
    Busy,
    /// Any other relay close code, said in `lpc_relay`'s words.
    Refused(RelayCloseCode),
    /// The socket to lightplayer.app never opened, or timed out.
    Unreachable,
}

impl RelayConnectFailure {
    /// Read the socket's words (`raw`, as `browser_websocket.js` says them).
    pub fn from_socket(raw: &str) -> Self {
        if raw.contains(RELAY_NO_HELD_KEY_PHRASE) {
            return Self::NoHeldKey;
        }
        match close_code(raw).and_then(RelayCloseCode::from_code) {
            Some(RelayCloseCode::BoardOffline) => Self::Offline,
            Some(RelayCloseCode::Busy) => Self::Busy,
            // A board turning the session away itself (its LAN rule, should
            // a relay ever pass it on) means the same.
            None if close_code(raw) == Some(1013) => Self::Busy,
            Some(code) => Self::Refused(code),
            None => Self::Unreachable,
        }
    }

    /// The sentence the card says.
    pub fn words(&self) -> String {
        match self {
            Self::NoHeldKey => RELAY_NO_HELD_KEY_WORDS.to_string(),
            Self::Offline => RELAY_OFFLINE_WORDS.to_string(),
            Self::Busy => WIFI_BUSY_WORDS.to_string(),
            Self::Refused(code) => code.words().to_string(),
            Self::Unreachable => RELAY_UNREACHABLE_WORDS.to_string(),
        }
    }
}

/// The close code in the socket's words (`… (code 4404: board-offline)`).
fn close_code(raw: &str) -> Option<u16> {
    let (_, after) = raw.split_once("code ")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_held_key_says_sign_in_and_plug_it_in_once() {
        let failure = RelayConnectFailure::from_socket(
            "relay link lost: no key this browser holds opens this board",
        );
        assert_eq!(failure, RelayConnectFailure::NoHeldKey);
        assert_eq!(
            failure.words(),
            "Sign in to Studio and plug this board in once to reach it through lightplayer.app."
        );
    }

    #[test]
    fn the_phrase_is_the_providers() {
        #[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
        assert_eq!(
            RELAY_NO_HELD_KEY_PHRASE,
            lpa_link::providers::browser_websocket::RELAY_NO_HELD_KEY
        );
        // Host builds carry no provider; the phrase is pinned by value.
        assert_eq!(
            RELAY_NO_HELD_KEY_PHRASE,
            "no key this browser holds opens this board"
        );
    }

    #[test]
    fn an_offline_board_says_it_isnt_online() {
        let failure = RelayConnectFailure::from_socket(
            "relay link lost: the board closed the link (code 4404: board-offline)",
        );
        assert_eq!(failure, RelayConnectFailure::Offline);
        assert_eq!(failure.words(), "The board isn't online.");
    }

    #[test]
    fn a_busy_board_says_try_again() {
        for raw in [
            "relay link lost: the board closed the link (code 4429: busy)",
            "relay link lost: the board closed the link (code 1013: try again later)",
        ] {
            let failure = RelayConnectFailure::from_socket(raw);
            assert_eq!(failure, RelayConnectFailure::Busy, "{raw}");
            assert_eq!(
                failure.words(),
                "Busy with another connection \u{2014} try again"
            );
        }
    }

    #[test]
    fn the_relays_other_refusals_keep_its_own_words() {
        let cases = [
            (4401, RelayCloseCode::SignInRequired),
            (4420, RelayCloseCode::SlowDown),
            (4410, RelayCloseCode::BoardGone),
            (1001, RelayCloseCode::GoingAway),
        ];
        for (code, close) in cases {
            let raw = format!(
                "relay link lost: the board closed the link (code {code}: {})",
                close.reason()
            );
            let failure = RelayConnectFailure::from_socket(&raw);
            assert_eq!(failure, RelayConnectFailure::Refused(close), "{raw}");
            assert_eq!(failure.words(), close.words());
        }
        assert_eq!(
            RelayConnectFailure::from_socket(
                "relay link lost: the board closed the link (code 4401: sign-in-required)"
            )
            .words(),
            "Sign in to lightplayer.app to reach boards through it."
        );
    }

    #[test]
    fn a_socket_that_never_opened_could_not_reach_the_relay() {
        for raw in [
            "relay connect timed out after 10 s",
            "relay connect to wss://lightplayer.app/relay/board/a0f26287b48c failed",
        ] {
            let failure = RelayConnectFailure::from_socket(raw);
            assert_eq!(failure, RelayConnectFailure::Unreachable, "{raw}");
            assert_eq!(
                failure.words(),
                "Couldn't reach lightplayer.app. Are you online?"
            );
        }
    }
}

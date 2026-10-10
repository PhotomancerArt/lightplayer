//! The connects to a Wi‑Fi board someone asked for, while they run and once
//! they fail: what the card ("Connect over Wi‑Fi", "Connect through
//! lightplayer.app") and Connect a board's Network row say under their
//! button.
//!
//! One attempt per target: a known board on the LAN or through the relay
//! (by MAC, so the answer finds its card even if the roster merged the entry
//! meanwhile), or Connect a board's one address field. A new press replaces the
//! target's attempt; a success clears it (the board's card takes over from
//! there); a failure stays, in plain words, until the next press.

use std::collections::BTreeMap;

use lpa_devices::BoardKey;

use super::relay_connect_failure::RelayConnectFailure;
use super::wifi_connect_failure::WifiConnectFailure;

/// Whose connect it is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WifiConnectTarget {
    /// A known board's card, on the LAN, by its MAC.
    Board(BoardKey),
    /// A known board's card, through lightplayer.app's relay, by its MAC.
    Relay(BoardKey),
    /// Connect a board's address field.
    Address,
}

/// One connect, under way or failed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct WifiConnectAttempt {
    /// The host (and port) dialled, as the words name it.
    host: String,
    /// Why it failed, in the card's words.
    failure: Option<String>,
    /// It was turned away because another device holds the board's one
    /// network connection.
    busy: bool,
}

/// What a card or Connect a board says about its connect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiWifiConnect {
    /// The address being reached: `192.168.1.40`, `lp-1a2b.local`; through
    /// the relay, `lightplayer.app`.
    pub host: String,
    /// Reached through lightplayer.app's relay rather than on the LAN.
    pub through_relay: bool,
    /// Under way: the button waits.
    pub connecting: bool,
    /// Why the last one failed, in plain words.
    pub error: Option<String>,
    /// The last one was turned away because another device holds the
    /// board's one network connection (the card's "Someone else
    /// connected"), rather than failing.
    pub busy: bool,
}

/// Every connect to a Wi‑Fi board this page asked for and has not seen
/// succeed.
#[derive(Clone, Debug, Default)]
pub struct WifiConnects {
    attempts: BTreeMap<WifiConnectTarget, WifiConnectAttempt>,
}

impl WifiConnects {
    /// A connect to `host` for `target` started.
    pub fn start(&mut self, target: WifiConnectTarget, host: &str) {
        self.attempts.insert(
            target,
            WifiConnectAttempt {
                host: host.to_string(),
                failure: None,
                busy: false,
            },
        );
    }

    /// `target`'s connect never reached a socket, or its socket answered:
    /// the failure stays until the next press; a success clears it.
    pub fn finish(
        &mut self,
        target: WifiConnectTarget,
        host: &str,
        result: Result<(), WifiConnectFailure>,
    ) {
        self.finish_with_words(
            target,
            host,
            result.map_err(|failure| {
                let busy = matches!(failure, WifiConnectFailure::Busy);
                (failure.words(), busy)
            }),
        );
    }

    /// A connect through the relay ended: as [`Self::finish`].
    pub fn finish_relay(
        &mut self,
        target: WifiConnectTarget,
        host: &str,
        result: Result<(), RelayConnectFailure>,
    ) {
        self.finish_with_words(
            target,
            host,
            result.map_err(|failure| {
                let busy = matches!(failure, RelayConnectFailure::Busy);
                (failure.words(), busy)
            }),
        );
    }

    fn finish_with_words(
        &mut self,
        target: WifiConnectTarget,
        host: &str,
        result: Result<(), (String, bool)>,
    ) {
        match result {
            Ok(()) => {
                self.attempts.remove(&target);
            }
            Err((words, busy)) => {
                self.attempts.insert(
                    target,
                    WifiConnectAttempt {
                        host: host.to_string(),
                        failure: Some(words),
                        busy,
                    },
                );
            }
        }
    }

    /// Whether `target`'s connect is under way.
    pub fn connecting(&self, target: WifiConnectTarget) -> bool {
        self.attempts
            .get(&target)
            .is_some_and(|attempt| attempt.failure.is_none())
    }

    /// Drop what a target said.
    pub fn forget(&mut self, target: WifiConnectTarget) {
        self.attempts.remove(&target);
    }

    /// Drop what either of `board`'s connects said (Forget took the board,
    /// or a connect on the other road began).
    pub fn forget_board(&mut self, board: BoardKey) {
        self.forget(WifiConnectTarget::Board(board));
        self.forget(WifiConnectTarget::Relay(board));
    }

    /// What `target`'s card or slot says.
    pub fn view(&self, target: WifiConnectTarget) -> Option<UiWifiConnect> {
        let attempt = self.attempts.get(&target)?;
        Some(UiWifiConnect {
            host: attempt.host.clone(),
            through_relay: matches!(target, WifiConnectTarget::Relay(_)),
            connecting: attempt.failure.is_none(),
            error: attempt.failure.clone(),
            busy: attempt.busy,
        })
    }

    /// What `board`'s card says: its last connect, whichever road it took
    /// (a press on one road drops what the other said, so there is at most
    /// one).
    pub fn view_board(&self, board: BoardKey) -> Option<UiWifiConnect> {
        self.view(WifiConnectTarget::Board(board))
            .or_else(|| self.view(WifiConnectTarget::Relay(board)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connect_waits_then_clears_on_success_or_keeps_its_failure() {
        let mut connects = WifiConnects::default();
        let board = WifiConnectTarget::Board(BoardKey::parse("a0f26287b48c").unwrap());
        connects.start(board, "10.0.0.5");
        assert!(connects.connecting(board));
        assert_eq!(
            connects.view(board),
            Some(UiWifiConnect {
                host: "10.0.0.5".to_string(),
                through_relay: false,
                connecting: true,
                error: None,
                busy: false
            })
        );
        connects.finish(board, "10.0.0.5", Ok(()));
        assert_eq!(connects.view(board), None);

        connects.start(WifiConnectTarget::Address, "10.0.0.9");
        connects.finish(
            WifiConnectTarget::Address,
            "10.0.0.9",
            Err(WifiConnectFailure::Busy),
        );
        let view = connects.view(WifiConnectTarget::Address).unwrap();
        assert!(!view.connecting);
        assert_eq!(
            view.error.as_deref(),
            Some("Busy with another connection \u{2014} try again")
        );
        assert!(!connects.connecting(WifiConnectTarget::Address));
        assert_eq!(connects.view(board), None, "one target's answer is its own");
    }

    #[test]
    fn a_relay_connect_says_its_own_words_and_the_card_reads_either_road() {
        let mut connects = WifiConnects::default();
        let key = BoardKey::parse("a0f26287b48c").unwrap();
        let relay = WifiConnectTarget::Relay(key);
        connects.start(relay, "lightplayer.app");
        assert!(connects.connecting(relay));
        assert!(!connects.connecting(WifiConnectTarget::Board(key)));
        connects.finish_relay(relay, "lightplayer.app", Err(RelayConnectFailure::Offline));
        let said = connects.view_board(key).expect("the card says why");
        assert!(said.through_relay && !said.connecting);
        assert_eq!(said.error.as_deref(), Some("The board isn't online."));
        connects.forget_board(key);
        assert_eq!(connects.view_board(key), None);
    }
}

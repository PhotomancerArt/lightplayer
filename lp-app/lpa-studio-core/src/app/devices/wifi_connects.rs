//! The connects to a Wi‑Fi board someone asked for, while they run and once
//! they fail: what the card ("Connect over Wi‑Fi", "Connect through
//! lightplayer.app") and Connect a board's Network row say under their
//! button.
//!
//! One attempt per target: a known board on the LAN or through the relay
//! (by MAC, so the answer finds its card even if the roster merged the entry
//! meanwhile), or Connect a board's one address field. A new press replaces the
//! target's attempt. A card's connect is under way from the press until the
//! board says who it is: the socket answering (through the relay, its secure
//! link up) is half-way, and the card keeps saying "Connecting…" until a link
//! of this tab reaches the board, or [`CONNECT_INTENT_GRACE`] passes with no
//! hello, when it says [`CONNECT_GAVE_UP`] (a held Connect's words). Before,
//! the socket's answer cleared it, and the card offered its Connect again
//! while the board was still being identified — the press looked lost
//! (`docs/defects/2026-10-25-a-relay-connect-looked-like-it-needed-two-presses.md`).
//! The address field's connect ends at the socket's answer: the new board's
//! own card takes over. A failure stays, in plain words, until the next
//! press — or until the board is reached by another road (plugged in, say),
//! when it no longer says anything true.

use std::collections::BTreeMap;

use lpa_devices::BoardKey;

use super::relay_connect_failure::RelayConnectFailure;
use super::wifi_connect_failure::WifiConnectFailure;
use crate::{CONNECT_GAVE_UP, CONNECT_INTENT_GRACE};

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
#[derive(Clone, Debug, PartialEq)]
struct WifiConnectAttempt {
    /// The host (and port) dialled, as the words name it.
    host: String,
    /// Why it failed, in the card's words.
    failure: Option<String>,
    /// It was turned away because another device holds the board's one
    /// network connection.
    busy: bool,
    /// When the socket answered (epoch seconds): the board is being
    /// identified, and the card still waits for its hello.
    answered_at: Option<f64>,
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
                answered_at: None,
            },
        );
    }

    /// `target`'s connect never reached a socket, or its socket answered
    /// at `now`: the failure stays until the next press; a card's success
    /// waits for the board's hello ([`Self::settle`]), the address field's
    /// clears.
    pub fn finish(
        &mut self,
        target: WifiConnectTarget,
        host: &str,
        result: Result<(), WifiConnectFailure>,
        now: f64,
    ) {
        self.finish_with_words(
            target,
            host,
            now,
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
        now: f64,
    ) {
        self.finish_with_words(
            target,
            host,
            now,
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
        now: f64,
        result: Result<(), (String, bool)>,
    ) {
        match (result, target) {
            (Ok(()), WifiConnectTarget::Address) => {
                self.attempts.remove(&target);
            }
            (Ok(()), _) => {
                self.attempts.insert(
                    target,
                    WifiConnectAttempt {
                        host: host.to_string(),
                        failure: None,
                        busy: false,
                        answered_at: Some(now),
                    },
                );
            }
            (Err((words, busy)), _) => {
                self.attempts.insert(
                    target,
                    WifiConnectAttempt {
                        host: host.to_string(),
                        failure: Some(words),
                        busy,
                        answered_at: None,
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

    /// Settle every card's connect against what `reached` says a link of
    /// this tab reaches at `now`. A board reached says nothing more: its
    /// failure goes (plugged in after its Wi‑Fi connect failed, it is not
    /// unreachable), and its connect is over, whether or not its socket's
    /// answer has come back yet (the board said hello, and its card takes
    /// over; an answer that comes later is settled as it lands). A connect
    /// whose socket answered and whose board has not said hello within
    /// [`CONNECT_INTENT_GRACE`] gives up, in a held Connect's words. Whether
    /// anything changed.
    pub fn settle(&mut self, reached: impl Fn(BoardKey) -> bool, now: f64) -> bool {
        let before = self.attempts.clone();
        self.attempts.retain(|target, _| match target {
            WifiConnectTarget::Board(board) | WifiConnectTarget::Relay(board) => !reached(*board),
            WifiConnectTarget::Address => true,
        });
        for attempt in self.attempts.values_mut() {
            let late = attempt
                .answered_at
                .is_some_and(|at| now - at >= CONNECT_INTENT_GRACE.as_secs_f64());
            if late {
                attempt.answered_at = None;
                attempt.failure = Some(CONNECT_GAVE_UP.to_string());
            }
        }
        self.attempts != before
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
        connects.finish(board, "10.0.0.5", Ok(()), 1.0);
        assert!(
            connects.connecting(board),
            "the socket answered: the card waits for the board's hello"
        );
        assert!(!connects.settle(|_| false, 2.0), "no hello yet");
        assert!(connects.settle(|_| true, 2.0), "the board said hello");
        assert_eq!(connects.view(board), None);

        connects.start(WifiConnectTarget::Address, "10.0.0.9");
        connects.finish(
            WifiConnectTarget::Address,
            "10.0.0.9",
            Err(WifiConnectFailure::Busy),
            1.0,
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
    fn a_failure_goes_once_its_board_is_reached_and_so_does_a_connect_under_way() {
        let mut connects = WifiConnects::default();
        let failed = BoardKey::parse("a0f26287b48c").unwrap();
        let dialling = BoardKey::parse("a0f26287b48d").unwrap();
        connects.start(WifiConnectTarget::Board(failed), "10.0.0.5");
        connects.finish(
            WifiConnectTarget::Board(failed),
            "10.0.0.5",
            Err(WifiConnectFailure::Busy),
            1.0,
        );
        connects.start(WifiConnectTarget::Relay(dialling), "lightplayer.app");
        connects.start(WifiConnectTarget::Address, "10.0.0.9");
        connects.finish(
            WifiConnectTarget::Address,
            "10.0.0.9",
            Err(WifiConnectFailure::Busy),
            1.0,
        );

        assert!(!connects.settle(|_| false, 2.0), "nothing reached");
        assert!(connects.settle(|board| board == failed, 2.0));
        assert_eq!(connects.view_board(failed), None, "reached: no failure");
        assert!(
            connects.connecting(WifiConnectTarget::Relay(dialling)),
            "not reached yet: still dialling"
        );
        assert!(connects.settle(|_| true, 2.0));
        assert_eq!(
            connects.view_board(dialling),
            None,
            "the board said hello before its socket's answer: the connect is over"
        );
        assert!(
            connects.view(WifiConnectTarget::Address).is_some(),
            "the address field's answer is no board's"
        );
    }

    #[test]
    fn a_relay_connect_says_its_own_words_and_the_card_reads_either_road() {
        let mut connects = WifiConnects::default();
        let key = BoardKey::parse("a0f26287b48c").unwrap();
        let relay = WifiConnectTarget::Relay(key);
        connects.start(relay, "lightplayer.app");
        assert!(connects.connecting(relay));
        assert!(!connects.connecting(WifiConnectTarget::Board(key)));
        connects.finish_relay(
            relay,
            "lightplayer.app",
            Err(RelayConnectFailure::Offline),
            1.0,
        );
        let said = connects.view_board(key).expect("the card says why");
        assert!(said.through_relay && !said.connecting);
        assert_eq!(said.error.as_deref(), Some("The board isn't online."));
        connects.forget_board(key);
        assert_eq!(connects.view_board(key), None);
    }

    #[test]
    fn an_answered_connect_waits_for_the_hello_then_gives_up_after_the_grace() {
        let mut connects = WifiConnects::default();
        let key = BoardKey::parse("a0f26287b48c").unwrap();
        let relay = WifiConnectTarget::Relay(key);
        connects.start(relay, "lightplayer.app");
        connects.finish_relay(relay, "lightplayer.app", Ok(()), 10.0);
        let grace = CONNECT_INTENT_GRACE.as_secs_f64();
        assert!(!connects.settle(|_| false, 10.0 + grace - 1.0));
        assert!(connects.connecting(relay), "still identifying the board");
        assert!(connects.settle(|_| false, 10.0 + grace));
        let said = connects.view_board(key).expect("the card says why");
        assert!(!said.connecting);
        assert_eq!(said.error.as_deref(), Some(CONNECT_GAVE_UP));

        // The address field's connect ends at the socket's answer: the new
        // board's own card takes over.
        connects.start(WifiConnectTarget::Address, "10.0.0.9");
        connects.finish(WifiConnectTarget::Address, "10.0.0.9", Ok(()), 1.0);
        assert_eq!(connects.view(WifiConnectTarget::Address), None);
    }
}

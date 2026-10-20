//! The move of a board's session from lightplayer.app onto Wi‑Fi, and back
//! (pure: no clock, no sockets; the controller's half is
//! `studio_controller/lan_move_flow.rs`).
//!
//! A board reached through lightplayer.app whose account list says it shares
//! this network moves onto the LAN by itself, once per page load:
//!
//! ```text
//! (eligible) ──▶ Dialling ──answered──▶ Binding ──its link is the LAN's──▶ Moved
//!                   │ refused              │ not within 8 s                 │ the LAN link went
//!                   ▼                      ▼                                ▼
//!                Ended(Stayed)          Ended(Stayed)          Lost{since} ──back within 5 s──▶ Moved
//!                                                                │ 5 s, not taken, still listed
//!                                                                ▼
//!                                                             Ended(Returned)   (through lightplayer.app again)
//! ```
//!
//! A board that has an entry here was tried this page load and is never
//! tried again; one the controller found busy (an update, a push, an open in
//! flight) has no entry yet, so waiting does not spend its one try. A
//! Wi‑Fi link closed with code 1000 was taken by another connection with the
//! same key ([`LanMoves::note_taken`]): going back through lightplayer.app
//! would only take it back, so the page does not.

use std::collections::{BTreeMap, BTreeSet};

use lpa_devices::BoardKey;

/// How long a LAN connection that answered has to become the board's own
/// link before the move gives up and stays on lightplayer.app.
pub const LAN_MOVE_BIND_SECS: f64 = 8.0;

/// How long a moved board's lost Wi‑Fi link has to come back before the page
/// goes back through lightplayer.app.
pub const LAN_MOVE_RETURN_SECS: f64 = 5.0;

/// Where one board's move stands.
#[derive(Clone, Debug, PartialEq)]
pub enum LanMoveStage {
    /// The LAN connection to `url` is being dialled.
    Dialling { url: String },
    /// The LAN connection answered at `since`; the board's own link is not
    /// the LAN's yet.
    Binding { url: String, since: f64 },
    /// The board's session is on Wi‑Fi at `url`.
    Moved { url: String },
    /// The moved board's Wi‑Fi link went at `since`.
    Lost { url: String, since: f64 },
    /// Over for this page load.
    Ended(LanMoveEnd),
}

/// How a move ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanMoveEnd {
    /// Wi‑Fi did not answer, or never became the board's link: it stayed on
    /// lightplayer.app.
    Stayed,
    /// Wi‑Fi dropped and the page went back through lightplayer.app.
    Returned,
    /// Another connection with the same key took the board's Wi‑Fi slot.
    Taken,
    /// Wi‑Fi dropped while the account's list no longer names the board
    /// online: nothing to go back through.
    Unlisted,
    /// The board is reached another way now (its cable, Bluetooth).
    Elsewhere,
}

/// What the controller sees of a board in one look.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LanMoveLook {
    /// The board's own link is the LAN connection the move dialled, ready
    /// and holding a tier.
    pub on_lan: bool,
    /// A link that is neither the LAN's nor lightplayer.app's reaches it
    /// (its cable, Bluetooth).
    pub reached_otherwise: bool,
    /// The account's list names the board online at lightplayer.app.
    pub listed: bool,
}

/// What the controller does after a look.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LanMoveStep {
    /// Nothing now.
    Wait,
    /// The board's link is the LAN's: move the editor onto it and let the
    /// relay session go.
    Bound { url: String },
    /// The LAN connection never became the board's link: let it go, and
    /// stay on lightplayer.app.
    GaveUp { url: String },
    /// The moved board's Wi‑Fi link went; it has a while to come back.
    Lost,
    /// It came back.
    Back,
    /// It did not: let the LAN session go and reach the board through
    /// lightplayer.app again.
    Return { url: String },
    /// It did not, and the page does not go back (`why`).
    NoReturn { url: String, why: LanMoveEnd },
    /// The board is reached another way: the move is over.
    Over,
}

/// Every board's move this page load, by MAC.
#[derive(Clone, Debug, Default)]
pub struct LanMoves {
    boards: BTreeMap<BoardKey, LanMoveStage>,
    /// Moved boards whose Wi‑Fi link was closed with code 1000.
    taken: BTreeSet<BoardKey>,
}

impl LanMoves {
    /// Whether `key` was tried this page load (it is never tried again).
    pub fn tried(&self, key: BoardKey) -> bool {
        self.boards.contains_key(&key)
    }

    /// Where `key`'s move stands.
    pub fn stage(&self, key: BoardKey) -> Option<&LanMoveStage> {
        self.boards.get(&key)
    }

    /// The boards whose move is not over.
    pub fn active(&self) -> Vec<BoardKey> {
        self.boards
            .iter()
            .filter(|(_, stage)| !matches!(stage, LanMoveStage::Ended(_)))
            .map(|(key, _)| *key)
            .collect()
    }

    /// The move of `key` starts: its LAN connection to `url` is dialled.
    pub fn start(&mut self, key: BoardKey, url: &str) {
        self.boards.insert(
            key,
            LanMoveStage::Dialling {
                url: url.to_string(),
            },
        );
    }

    /// Whether `key`'s LAN connection is being dialled for a move (its
    /// answer is the move's, not a card's).
    pub fn dialling(&self, key: BoardKey) -> bool {
        matches!(self.boards.get(&key), Some(LanMoveStage::Dialling { .. }))
    }

    /// The move's LAN dial answered (`ok`) or did not, at `now`.
    pub fn dialled(&mut self, key: BoardKey, ok: bool, now: f64) {
        let Some(LanMoveStage::Dialling { url }) = self.boards.get(&key) else {
            return;
        };
        let next = match ok {
            true => LanMoveStage::Binding {
                url: url.clone(),
                since: now,
            },
            false => LanMoveStage::Ended(LanMoveEnd::Stayed),
        };
        self.boards.insert(key, next);
    }

    /// The moved board's Wi‑Fi link was closed with code 1000: another
    /// connection with the same key took the board's slot.
    pub fn note_taken(&mut self, key: BoardKey) {
        if matches!(
            self.boards.get(&key),
            Some(LanMoveStage::Moved { .. } | LanMoveStage::Lost { .. })
        ) {
            self.taken.insert(key);
        }
    }

    /// The URL `key`'s move dialled, while it is not over.
    pub fn url(&self, key: BoardKey) -> Option<&str> {
        match self.boards.get(&key)? {
            LanMoveStage::Dialling { url }
            | LanMoveStage::Binding { url, .. }
            | LanMoveStage::Moved { url }
            | LanMoveStage::Lost { url, .. } => Some(url),
            LanMoveStage::Ended(_) => None,
        }
    }

    /// Look at `key` at `now`, as `look` says it stands: the move's next
    /// stage, and what the controller does about it.
    pub fn look(&mut self, key: BoardKey, look: LanMoveLook, now: f64) -> LanMoveStep {
        let Some(stage) = self.boards.get(&key).cloned() else {
            return LanMoveStep::Wait;
        };
        let taken = self.taken.contains(&key);
        let (next, step) = match stage {
            LanMoveStage::Dialling { .. } | LanMoveStage::Ended(_) => return LanMoveStep::Wait,
            LanMoveStage::Binding { url, .. } if look.on_lan => (
                LanMoveStage::Moved { url: url.clone() },
                LanMoveStep::Bound { url },
            ),
            LanMoveStage::Binding { url, since } if now - since >= LAN_MOVE_BIND_SECS => (
                LanMoveStage::Ended(LanMoveEnd::Stayed),
                LanMoveStep::GaveUp { url },
            ),
            LanMoveStage::Binding { .. } => return LanMoveStep::Wait,
            LanMoveStage::Moved { .. } | LanMoveStage::Lost { .. } if look.on_lan => {
                self.taken.remove(&key);
                let back = matches!(stage, LanMoveStage::Lost { .. });
                let url = self.url(key).unwrap_or_default().to_string();
                (
                    LanMoveStage::Moved { url },
                    match back {
                        true => LanMoveStep::Back,
                        false => LanMoveStep::Wait,
                    },
                )
            }
            LanMoveStage::Moved { .. } | LanMoveStage::Lost { .. } if look.reached_otherwise => (
                LanMoveStage::Ended(LanMoveEnd::Elsewhere),
                LanMoveStep::Over,
            ),
            LanMoveStage::Moved { url } | LanMoveStage::Lost { url, .. } if taken => (
                LanMoveStage::Ended(LanMoveEnd::Taken),
                LanMoveStep::NoReturn {
                    url,
                    why: LanMoveEnd::Taken,
                },
            ),
            LanMoveStage::Moved { url } => {
                (LanMoveStage::Lost { url, since: now }, LanMoveStep::Lost)
            }
            LanMoveStage::Lost { url, .. } if !look.listed => (
                LanMoveStage::Ended(LanMoveEnd::Unlisted),
                LanMoveStep::NoReturn {
                    url,
                    why: LanMoveEnd::Unlisted,
                },
            ),
            LanMoveStage::Lost { url, since } if now - since >= LAN_MOVE_RETURN_SECS => (
                LanMoveStage::Ended(LanMoveEnd::Returned),
                LanMoveStep::Return { url },
            ),
            LanMoveStage::Lost { .. } => return LanMoveStep::Wait,
        };
        self.boards.insert(key, next);
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "ws://192.168.4.20:80/link";

    #[test]
    fn a_move_dials_binds_and_is_moved() {
        let mut moves = LanMoves::default();
        let key = key();
        assert!(!moves.tried(key));
        moves.start(key, URL);
        assert!(moves.tried(key) && moves.dialling(key));
        assert_eq!(moves.look(key, on_relay(), 0.0), LanMoveStep::Wait);
        moves.dialled(key, true, 1.0);
        assert_eq!(moves.look(key, on_relay(), 2.0), LanMoveStep::Wait);
        assert_eq!(
            moves.look(key, on_lan(), 3.0),
            LanMoveStep::Bound {
                url: URL.to_string()
            }
        );
        assert_eq!(
            moves.stage(key),
            Some(&LanMoveStage::Moved {
                url: URL.to_string()
            })
        );
        assert_eq!(moves.look(key, on_lan(), 4.0), LanMoveStep::Wait);
    }

    #[test]
    fn a_refused_dial_stays_and_is_never_tried_again() {
        let mut moves = LanMoves::default();
        let key = key();
        moves.start(key, URL);
        moves.dialled(key, false, 1.0);
        assert_eq!(
            moves.stage(key),
            Some(&LanMoveStage::Ended(LanMoveEnd::Stayed))
        );
        assert!(moves.tried(key));
        assert!(moves.active().is_empty());
        assert_eq!(moves.look(key, on_lan(), 2.0), LanMoveStep::Wait);
    }

    #[test]
    fn a_dial_that_never_binds_gives_up_at_eight_seconds() {
        let mut moves = LanMoves::default();
        let key = key();
        moves.start(key, URL);
        moves.dialled(key, true, 10.0);
        assert_eq!(moves.look(key, on_relay(), 17.9), LanMoveStep::Wait);
        assert_eq!(
            moves.look(key, on_relay(), 18.0),
            LanMoveStep::GaveUp {
                url: URL.to_string()
            }
        );
        assert_eq!(
            moves.stage(key),
            Some(&LanMoveStage::Ended(LanMoveEnd::Stayed))
        );
    }

    #[test]
    fn a_lost_link_returns_at_five_seconds_and_only_once() {
        let mut moves = moved();
        let key = key();
        assert_eq!(moves.look(key, nowhere(), 100.0), LanMoveStep::Lost);
        assert_eq!(moves.look(key, nowhere(), 104.9), LanMoveStep::Wait);
        assert_eq!(
            moves.look(key, nowhere(), 105.0),
            LanMoveStep::Return {
                url: URL.to_string()
            }
        );
        assert_eq!(
            moves.stage(key),
            Some(&LanMoveStage::Ended(LanMoveEnd::Returned))
        );
        assert_eq!(moves.look(key, nowhere(), 200.0), LanMoveStep::Wait);
        assert!(moves.tried(key), "never moves again this page load");
    }

    #[test]
    fn a_link_back_within_five_seconds_keeps_the_move() {
        let mut moves = moved();
        let key = key();
        assert_eq!(moves.look(key, nowhere(), 100.0), LanMoveStep::Lost);
        assert_eq!(moves.look(key, on_lan(), 103.0), LanMoveStep::Back);
        assert_eq!(
            moves.stage(key),
            Some(&LanMoveStage::Moved {
                url: URL.to_string()
            })
        );
        // A later loss starts its own five seconds.
        assert_eq!(moves.look(key, nowhere(), 110.0), LanMoveStep::Lost);
        assert_eq!(moves.look(key, nowhere(), 114.0), LanMoveStep::Wait);
    }

    #[test]
    fn a_link_taken_with_code_1000_does_not_go_back() {
        let mut moves = moved();
        let key = key();
        moves.note_taken(key);
        assert_eq!(
            moves.look(key, nowhere(), 100.0),
            LanMoveStep::NoReturn {
                url: URL.to_string(),
                why: LanMoveEnd::Taken
            }
        );
        assert_eq!(moves.look(key, nowhere(), 200.0), LanMoveStep::Wait);
    }

    #[test]
    fn a_taken_note_on_a_board_that_did_not_move_is_nothing() {
        let mut moves = LanMoves::default();
        let key = key();
        moves.note_taken(key);
        moves.start(key, URL);
        moves.dialled(key, true, 0.0);
        moves.look(key, on_lan(), 1.0);
        assert_eq!(moves.look(key, nowhere(), 2.0), LanMoveStep::Lost);
    }

    #[test]
    fn a_board_no_longer_listed_does_not_go_back() {
        let mut moves = moved();
        let key = key();
        assert_eq!(moves.look(key, nowhere(), 100.0), LanMoveStep::Lost);
        let unlisted = LanMoveLook {
            listed: false,
            ..nowhere()
        };
        assert_eq!(
            moves.look(key, unlisted, 101.0),
            LanMoveStep::NoReturn {
                url: URL.to_string(),
                why: LanMoveEnd::Unlisted
            }
        );
    }

    #[test]
    fn a_board_reached_another_way_ends_the_move() {
        let mut moves = moved();
        let key = key();
        let usb = LanMoveLook {
            reached_otherwise: true,
            ..nowhere()
        };
        assert_eq!(moves.look(key, usb, 100.0), LanMoveStep::Over);
        assert_eq!(
            moves.stage(key),
            Some(&LanMoveStage::Ended(LanMoveEnd::Elsewhere))
        );
    }

    fn moved() -> LanMoves {
        let mut moves = LanMoves::default();
        let key = key();
        moves.start(key, URL);
        moves.dialled(key, true, 0.0);
        assert!(matches!(
            moves.look(key, on_lan(), 1.0),
            LanMoveStep::Bound { .. }
        ));
        moves
    }

    fn on_relay() -> LanMoveLook {
        LanMoveLook {
            on_lan: false,
            reached_otherwise: false,
            listed: true,
        }
    }

    fn on_lan() -> LanMoveLook {
        LanMoveLook {
            on_lan: true,
            ..on_relay()
        }
    }

    fn nowhere() -> LanMoveLook {
        on_relay()
    }

    fn key() -> BoardKey {
        BoardKey::parse("a0f26287b49a").unwrap()
    }
}

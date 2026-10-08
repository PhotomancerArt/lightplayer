//! The connects over Wi‑Fi someone asked for, while they run and once they
//! fail: what the card ("Connect over Wi‑Fi") and the add slot ("Connect a
//! board on Wi‑Fi") say under their button.
//!
//! One attempt per target: a known board (by MAC, so the answer finds its
//! card even if the roster merged the entry meanwhile), or the add slot's
//! one field. A new press replaces the target's attempt; a success clears
//! it (the board's card takes over from there); a failure stays, in plain
//! words, until the next press.

use std::collections::BTreeMap;

use lpa_devices::BoardKey;

use super::wifi_connect_failure::WifiConnectFailure;

/// Whose connect it is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WifiConnectTarget {
    /// A known board's card, by its MAC.
    Board(BoardKey),
    /// The add slot's address field.
    Address,
}

/// One connect, under way or failed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct WifiConnectAttempt {
    /// The host (and port) dialled, as the words name it.
    host: String,
    failure: Option<WifiConnectFailure>,
}

/// What a card or the add slot says about its connect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiWifiConnect {
    /// The address being reached: `192.168.1.40`, `lp-1a2b.local`.
    pub host: String,
    /// Under way: the button waits.
    pub connecting: bool,
    /// Why the last one failed, in plain words.
    pub error: Option<String>,
}

/// Every connect over Wi‑Fi this page asked for and has not seen succeed.
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
        match result {
            Ok(()) => {
                self.attempts.remove(&target);
            }
            Err(failure) => {
                self.attempts.insert(
                    target,
                    WifiConnectAttempt {
                        host: host.to_string(),
                        failure: Some(failure),
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

    /// Drop what a board's card said (Forget took the board).
    pub fn forget(&mut self, target: WifiConnectTarget) {
        self.attempts.remove(&target);
    }

    /// What `target`'s card or slot says.
    pub fn view(&self, target: WifiConnectTarget) -> Option<UiWifiConnect> {
        let attempt = self.attempts.get(&target)?;
        Some(UiWifiConnect {
            host: attempt.host.clone(),
            connecting: attempt.failure.is_none(),
            error: attempt.failure.as_ref().map(WifiConnectFailure::words),
        })
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
                connecting: true,
                error: None
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
}

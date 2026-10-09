//! [`TakeOvers`]: every Connect-on-a-held-board under way in this tab, and
//! [`UiTakeOver`], the words its card shows meanwhile.
//!
//! A take-over runs in three stages:
//!
//! 1. **Asking** — the ask is posted; the holder has
//!    [`ASK_PATIENCE_SECS`] to answer ("That tab didn't answer" after it: a
//!    tab of an older build never answers, since the channel ignores notes
//!    of another version).
//! 2. **Opening** — the holder let go; this tab opens the ports the hold
//!    kept shut, and the board's own hello says which one is the board. It
//!    ends when the board is ready, or after [`OPEN_PATIENCE_SECS`] with
//!    "Still in use".
//! 3. **Failed** — the reason, until the next press or the board is ready.
//!
//! Like `WifiConnects`, a side map beside the roster: the card reads words
//! and a flag, never an action (the verb is the offer,
//! [`super::take_over_offer`]).

use std::collections::BTreeMap;

use lpa_devices::DeviceId;

use super::board_hold::HoldKey;
use super::take_over_offer::TAKE_OVER_ASKING;

/// How long the holder has to answer an ask.
pub const ASK_PATIENCE_SECS: f64 = 5.0;
/// How long the freed board has to open here and say hello.
pub const OPEN_PATIENCE_SECS: f64 = 10.0;

/// The card's words while opening the freed board.
pub const TAKE_OVER_OPENING_WORDS: &str = "Opening\u{2026}";
/// The holder never answered.
pub const TAKE_OVER_NO_ANSWER: &str = "That tab didn't answer";
/// The holder let go, and the board still did not open here.
pub const TAKE_OVER_STILL_IN_USE: &str = "Still in use";
/// The holder said it does not have the board, and another tab does.
pub const TAKE_OVER_ANOTHER_TAB: &str = "Another tab has it now";

/// What a card says about its take-over.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTakeOver {
    pub words: String,
    /// The take-over ended without the board: the words are why.
    pub failed: bool,
}

/// One take-over's stage.
#[derive(Clone, Debug, PartialEq)]
pub enum TakeOverStage {
    /// Ask number `request` for `key` is out, until `deadline`.
    Asking {
        request: u64,
        key: HoldKey,
        deadline: f64,
    },
    /// The holder let go; the board opens here until `deadline`.
    Opening { deadline: f64 },
    /// It did not work, and why.
    Failed(String),
}

/// Every take-over in this tab, by the device it is for.
#[derive(Clone, Debug, Default)]
pub struct TakeOvers {
    by_device: BTreeMap<DeviceId, TakeOverStage>,
    next_request: u64,
}

/// What a passed deadline did to one take-over.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TakeOverTimeout {
    /// Ask `request` went unanswered: give it up.
    NoAnswer { device: DeviceId, request: u64 },
    /// The freed board did not open in time.
    StillInUse { device: DeviceId },
}

impl TakeOvers {
    /// A fresh request number for an ask (the controller's own counter).
    pub fn mint_request(&mut self) -> u64 {
        self.next_request += 1;
        self.next_request
    }

    /// `device`'s ask number `request` for `key` is out, at `now`.
    pub fn ask(&mut self, device: DeviceId, request: u64, key: HoldKey, now: f64) {
        self.by_device.insert(
            device,
            TakeOverStage::Asking {
                request,
                key,
                deadline: now + ASK_PATIENCE_SECS,
            },
        );
    }

    /// Whether `device`'s holder is being asked right now.
    pub fn asking(&self, device: DeviceId) -> bool {
        matches!(
            self.by_device.get(&device),
            Some(TakeOverStage::Asking { .. })
        )
    }

    /// The device whose ask is number `request`, while it waits.
    pub fn device_asking(&self, request: u64) -> Option<DeviceId> {
        self.by_device
            .iter()
            .find_map(|(device, stage)| match stage {
                TakeOverStage::Asking { request: asked, .. } if *asked == request => Some(*device),
                _ => None,
            })
    }

    /// The devices (and their ask numbers) asking about `key`.
    pub fn asking_for(&self, key: &HoldKey) -> Vec<(DeviceId, u64)> {
        self.by_device
            .iter()
            .filter_map(|(device, stage)| match stage {
                TakeOverStage::Asking {
                    request,
                    key: asked,
                    ..
                } if asked == key => Some((*device, *request)),
                _ => None,
            })
            .collect()
    }

    /// The holder let go of `device`'s board: it opens here now.
    pub fn opening(&mut self, device: DeviceId, now: f64) {
        self.by_device.insert(
            device,
            TakeOverStage::Opening {
                deadline: now + OPEN_PATIENCE_SECS,
            },
        );
    }

    /// Whether `device`'s board is being opened after its holder let go.
    pub fn is_opening(&self, device: DeviceId) -> bool {
        matches!(
            self.by_device.get(&device),
            Some(TakeOverStage::Opening { .. })
        )
    }

    /// `device`'s take-over ended without the board, for `reason`.
    pub fn fail(&mut self, device: DeviceId, reason: impl Into<String>) {
        self.by_device
            .insert(device, TakeOverStage::Failed(reason.into()));
    }

    /// `device`'s board is ready here: nothing more to say.
    pub fn done(&mut self, device: DeviceId) {
        self.by_device.remove(&device);
    }

    /// The devices with a take-over in some stage.
    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.by_device.keys().copied()
    }

    /// Every deadline passed at `now`: an unanswered ask fails "That tab
    /// didn't answer", an open that never came fails "Still in use".
    pub fn expire(&mut self, now: f64) -> Vec<TakeOverTimeout> {
        let mut timeouts = Vec::new();
        for (device, stage) in self.by_device.iter_mut() {
            match stage {
                TakeOverStage::Asking {
                    request, deadline, ..
                } if now >= *deadline => {
                    timeouts.push(TakeOverTimeout::NoAnswer {
                        device: *device,
                        request: *request,
                    });
                    *stage = TakeOverStage::Failed(TAKE_OVER_NO_ANSWER.to_string());
                }
                TakeOverStage::Opening { deadline } if now >= *deadline => {
                    timeouts.push(TakeOverTimeout::StillInUse { device: *device });
                    *stage = TakeOverStage::Failed(TAKE_OVER_STILL_IN_USE.to_string());
                }
                _ => {}
            }
        }
        timeouts
    }

    /// What `device`'s card says, when it has a take-over to tell.
    pub fn view(&self, device: DeviceId) -> Option<UiTakeOver> {
        Some(match self.by_device.get(&device)? {
            TakeOverStage::Asking { .. } => UiTakeOver {
                words: TAKE_OVER_ASKING.to_string(),
                failed: false,
            },
            TakeOverStage::Opening { .. } => UiTakeOver {
                words: TAKE_OVER_OPENING_WORDS.to_string(),
                failed: false,
            },
            TakeOverStage::Failed(reason) => UiTakeOver {
                words: reason.clone(),
                failed: true,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::BoardKey;

    use super::*;
    use crate::app::devices::board_hold::UsbPair;

    #[test]
    fn an_ask_waits_then_opens_and_says_so() {
        let mut overs = TakeOvers::default();
        let request = overs.mint_request();
        overs.ask(DeviceId(4), request, key(), 100.0);

        assert!(overs.asking(DeviceId(4)));
        assert_eq!(overs.device_asking(request), Some(DeviceId(4)));
        assert_eq!(overs.asking_for(&key()), vec![(DeviceId(4), request)]);
        assert_eq!(
            overs.view(DeviceId(4)),
            Some(UiTakeOver {
                words: "Asking the other tab\u{2026}".to_string(),
                failed: false,
            })
        );

        overs.opening(DeviceId(4), 101.0);
        assert!(!overs.asking(DeviceId(4)) && overs.is_opening(DeviceId(4)));
        assert_eq!(overs.device_asking(request), None);
        assert_eq!(
            overs.view(DeviceId(4)).map(|view| view.words),
            Some("Opening\u{2026}".to_string())
        );

        overs.done(DeviceId(4));
        assert_eq!(overs.view(DeviceId(4)), None);
    }

    #[test]
    fn deadlines_fail_an_unanswered_ask_and_an_open_that_never_came() {
        let mut overs = TakeOvers::default();
        overs.ask(DeviceId(1), 7, key(), 100.0);
        overs.opening(DeviceId(2), 100.0);

        assert!(overs.expire(100.0 + ASK_PATIENCE_SECS - 0.01).is_empty());
        assert_eq!(
            overs.expire(100.0 + ASK_PATIENCE_SECS),
            vec![TakeOverTimeout::NoAnswer {
                device: DeviceId(1),
                request: 7
            }]
        );
        assert_eq!(
            overs.view(DeviceId(1)),
            Some(UiTakeOver {
                words: "That tab didn't answer".to_string(),
                failed: true,
            })
        );
        assert_eq!(
            overs.expire(100.0 + OPEN_PATIENCE_SECS),
            vec![TakeOverTimeout::StillInUse {
                device: DeviceId(2)
            }]
        );
        assert_eq!(
            overs.view(DeviceId(2)).map(|view| view.words),
            Some("Still in use".to_string())
        );
        assert!(overs.expire(1e9).is_empty(), "a failure does not expire");
    }

    #[test]
    fn request_numbers_are_the_controllers_own_and_never_repeat() {
        let mut overs = TakeOvers::default();
        assert_eq!(overs.mint_request(), 1);
        assert_eq!(overs.mint_request(), 2);
    }

    fn key() -> HoldKey {
        HoldKey::usb(
            BoardKey::parse("a0:f2:62:87:b4:8c").expect("a mac"),
            UsbPair {
                vendor: 0x303a,
                product: 0x1001,
            },
        )
    }
}

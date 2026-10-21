//! One board on one account's list: what the cloud keeps so a board that
//! ever joined shows on every computer the account signs in on, offline
//! included (the account board list ADR).

use alloc::string::String;
use alloc::vec::Vec;
use lpc_history::PrefixedUid;
use lpc_relay::RelayBoardId;

/// The most boards one account's list holds — the wire's one constant
/// (`lpc_cloud_api::MAX_ACCOUNT_BOARDS`), so the list and its answer cannot
/// disagree. A board new to a full list does not join it: the relay still
/// lets the board online, and a Studio report is refused by name. The cap
/// is the domain's rule, applied where a record is created; the store holds
/// whatever it is given.
pub use lpc_cloud_api::MAX_ACCOUNT_BOARDS;

/// One board on one account's list.
///
/// A record grants nothing: presence, pictures and routes stay gated by the
/// accounts the board proved to the relay. It is written only from that
/// proof (a relay registration) or from Studio holding the account's
/// current key on a live link to the board, and it is read only by its own
/// account. Only Forget removes it.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRecord {
    /// The account whose list this is (`usr…`).
    pub user: PrefixedUid,
    /// The board: its MAC.
    pub board: RelayBoardId,
    /// The board's name as the board reports it (never a browser's local
    /// rename).
    pub label: String,
    /// The hardware model, from a Studio report only: the relay's hello
    /// carries none, so a record the relay alone wrote has none.
    pub model: Option<String>,
    /// The firmware version last reported.
    pub firmware: Option<String>,
    /// The name of the project the board last reported playing; `None` when
    /// it reported none, or never reported.
    pub project: Option<String>,
    /// When the board joined this list, f64 epoch seconds.
    pub joined_at: f64,
    /// When each kind of link last saw the board.
    pub seen: BoardSightings,
    /// The board's last picture, written behind (never per picture).
    pub picture: Option<StoredPicture>,
}

/// When each kind of link last saw a board, f64 epoch seconds: the relay's
/// from the hub, the others from Studio's reports.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoardSightings {
    /// Online at the relay (registered, or the time it left).
    pub relay: Option<f64>,
    /// A Studio holding the account's key over USB.
    pub usb: Option<f64>,
    /// A Studio holding the account's key over Bluetooth.
    pub ble: Option<f64>,
    /// A Studio holding the account's key over the board's Wi‑Fi link.
    pub lan: Option<f64>,
}

impl BoardSightings {
    /// The newest sighting of any kind.
    #[must_use]
    pub fn last_seen(&self) -> Option<f64> {
        [self.relay, self.usb, self.ble, self.lan]
            .into_iter()
            .flatten()
            .reduce(f64::max)
    }

    /// Each kind's newer time of `self` and `newer`.
    pub fn absorb(&mut self, newer: BoardSightings) {
        fn latest(mine: &mut Option<f64>, theirs: Option<f64>) {
            *mine = match (*mine, theirs) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
        latest(&mut self.relay, newer.relay);
        latest(&mut self.usb, newer.usb);
        latest(&mut self.ble, newer.ble);
        latest(&mut self.lan, newer.lan);
    }
}

/// A board's last picture as the board list keeps it: what the relay's
/// picture cache held when it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPicture {
    /// When the picture arrived at the relay, f64 epoch seconds.
    pub at: f64,
    /// Lamps per output, in the project's tree order.
    pub outputs: Vec<u32>,
    /// R, G, B per sample.
    pub colors: Vec<u8>,
    /// The accounts that may read it: those the board proved when the
    /// picture was taken. A record grants nothing, so a list that holds the
    /// board does not make its picture readable on its own.
    pub accounts: Vec<PrefixedUid>,
}

/// A newer fact about a board, from one source (a relay registration, a
/// relay leave, a project change, or a Studio report), to fold into a
/// record by [`BoardRecord::absorb`].
///
/// Each field says what this source knows: `None` is "this source does not
/// say", never "clear it". The project's `Some(None)` is a fact (no project
/// is loaded); its `None` is silence (a board that has just registered has
/// not reported one yet).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoardFacts {
    /// The board's own name.
    pub label: Option<String>,
    /// The hardware model (Studio only).
    pub model: Option<String>,
    /// The firmware version.
    pub firmware: Option<String>,
    /// The project: `Some(None)` when none is loaded.
    pub project: Option<Option<String>>,
    /// The sightings this fact brings.
    pub seen: BoardSightings,
}

impl BoardRecord {
    /// A record for a board joining `user`'s list at `joined_at`, holding
    /// what `facts` says.
    #[must_use]
    pub fn joining(
        user: PrefixedUid,
        board: RelayBoardId,
        facts: BoardFacts,
        joined_at: f64,
    ) -> Self {
        let mut record = Self {
            user,
            board,
            label: String::new(),
            model: None,
            firmware: None,
            project: None,
            joined_at,
            seen: BoardSightings::default(),
            picture: None,
        };
        record.absorb(facts);
        record
    }

    /// Fold a newer fact in. Label, firmware and project take what the
    /// fact says; the model only when the fact carries one (the relay has
    /// none to give); each sighting keeps the newer time.
    pub fn absorb(&mut self, facts: BoardFacts) {
        let BoardFacts {
            label,
            model,
            firmware,
            project,
            seen,
        } = facts;
        if let Some(label) = label {
            self.label = label;
        }
        if model.is_some() {
            self.model = model;
        }
        if firmware.is_some() {
            self.firmware = firmware;
        }
        if let Some(project) = project {
            self.project = project;
        }
        self.seen.absorb(seen);
    }

    /// When any link last saw the board.
    #[must_use]
    pub fn last_seen(&self) -> Option<f64> {
        self.seen.last_seen()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_history::UidPrefix;

    #[test]
    fn last_seen_is_the_newest_sighting_of_any_kind() {
        let seen = BoardSightings {
            relay: Some(10.0),
            usb: Some(30.0),
            ble: None,
            lan: Some(20.0),
        };
        assert_eq!(seen.last_seen(), Some(30.0));
        assert_eq!(BoardSightings::default().last_seen(), None);
    }

    #[test]
    fn absorbing_overwrites_label_firmware_and_project() {
        let mut record = record();
        record.absorb(BoardFacts {
            label: Some("Porch".to_string()),
            firmware: Some("2026.10.20-1".to_string()),
            project: Some(Some("Rocaille".to_string())),
            ..BoardFacts::default()
        });
        assert_eq!(record.label, "Porch");
        assert_eq!(record.firmware.as_deref(), Some("2026.10.20-1"));
        assert_eq!(record.project.as_deref(), Some("Rocaille"));

        record.absorb(BoardFacts {
            project: Some(None),
            ..BoardFacts::default()
        });
        assert_eq!(record.project, None, "no project loaded is a fact");
        assert_eq!(record.label, "Porch", "silence keeps the label");
        assert_eq!(
            record.firmware.as_deref(),
            Some("2026.10.20-1"),
            "an unknown firmware keeps the last known one"
        );
    }

    #[test]
    fn the_model_changes_only_when_a_fact_carries_one() {
        let mut record = record();
        record.absorb(BoardFacts {
            model: Some("XIAO ESP32-C6".to_string()),
            ..BoardFacts::default()
        });
        record.absorb(BoardFacts {
            label: Some("Lamp".to_string()),
            ..BoardFacts::default()
        });
        assert_eq!(record.model.as_deref(), Some("XIAO ESP32-C6"));
    }

    #[test]
    fn each_sighting_keeps_the_newer_time() {
        let mut record = record();
        record.absorb(BoardFacts {
            seen: BoardSightings {
                relay: Some(50.0),
                usb: Some(5.0),
                ..BoardSightings::default()
            },
            ..BoardFacts::default()
        });
        record.absorb(BoardFacts {
            seen: BoardSightings {
                relay: Some(40.0),
                ble: Some(45.0),
                ..BoardSightings::default()
            },
            ..BoardFacts::default()
        });
        assert_eq!(
            record.seen,
            BoardSightings {
                relay: Some(50.0),
                usb: Some(5.0),
                ble: Some(45.0),
                lan: None,
            }
        );
        assert_eq!(record.last_seen(), Some(50.0));
    }

    #[test]
    fn joining_holds_the_first_facts() {
        let joined = BoardRecord::joining(
            user(),
            RelayBoardId([1; 6]),
            BoardFacts {
                label: Some("Lamp".to_string()),
                seen: BoardSightings {
                    relay: Some(7.0),
                    ..BoardSightings::default()
                },
                ..BoardFacts::default()
            },
            7.0,
        );
        assert_eq!(joined.label, "Lamp");
        assert_eq!(joined.joined_at, 7.0);
        assert_eq!(joined.last_seen(), Some(7.0));
        assert_eq!(joined.picture, None);
    }

    fn user() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[1u8; 16])
    }

    fn record() -> BoardRecord {
        BoardRecord::joining(
            user(),
            RelayBoardId([1; 6]),
            BoardFacts {
                label: Some("Lamp".to_string()),
                ..BoardFacts::default()
            },
            1.0,
        )
    }
}

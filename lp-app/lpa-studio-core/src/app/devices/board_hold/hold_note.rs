//! [`HoldNote`]: what Studio tabs of one browser tell each other about the
//! boards they hold, and how a note travels as text.
//!
//! Five notes:
//!
//! | note | said by | means |
//! |---|---|---|
//! | `Holds { key, level, locked }` | the holder | I hold this board (re-said when the level changes, when its lock becomes mine, and in answer to `Who`) |
//! | `Gone { key }` | the holder | I let it go |
//! | `Ask { request, key, holder }` | a tab that wants the board | please let go of it |
//! | `Answer { request, asker, outcome }` | the asked tab | `Released`, or refused and why |
//! | `Who` | a new tab | everyone, say what you hold |
//!
//! Every note rides in a JSON envelope `{"v":1,"tab":"<id>","note":…}`,
//! the note externally tagged (`{"holds":{"key":…,"level":…,"locked":true}}`,
//! `"who"`):
//! no internally tagged or flattened serde, which the repo's
//! `lint-serde-content` keeps out (`docs/adr/2026-07-04-json-only-artifacts.md`).
//! [`HoldNote::decode`] ignores a note of another version, malformed text,
//! and a note this tab sent itself. The version is not a compatibility
//! promise: tabs of different builds share one browser (lightplayer.app
//! redeploys daily and a tab can live for days), and the version is what
//! lets a fresh tab ignore a stale one rather than misread it. Nothing here
//! is persisted; there is no alias and no fallback.

use lpa_devices::HoldLevel;
use serde::{Deserialize, Serialize};

use super::hold_key::HoldKey;
use super::tab_id::TabId;

/// The hold channel's note version. A note of any other version is ignored.
pub const HOLD_PROTO_VERSION: u32 = 1;

/// One note on the hold channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldNote {
    /// I hold this board, at this level. Said after the claim, again on a
    /// level change, and in answer to [`Self::Who`].
    ///
    /// `locked`: the holder has the board's Web Lock. A tab can hold a
    /// board before its lock is free — its link is open here, and the lock
    /// is still another tab's (a board's network slot goes to the newest
    /// client, and the tab it left has not let the lock go yet; or a stale
    /// lock stands) — and then says `false`, and says `Holds` again with
    /// `true` once the lock is its own. Other tabs watch only a locked
    /// hold's lock (the sentinel): an unlocked hold's lock coming free says
    /// nothing about its holder.
    Holds {
        key: HoldKey,
        level: HoldLevel,
        locked: bool,
    },
    /// I let this board go (the port is closed and the lock released).
    Gone { key: HoldKey },
    /// Please let go of this board. `holder` names the tab asked, when the
    /// asker knows it: that tab answers even when it no longer holds the
    /// board (`NotHeld`), and every other tab stays quiet. With no holder
    /// named, only the tab that holds the board answers.
    Ask {
        request: u64,
        key: HoldKey,
        #[serde(default)]
        holder: Option<TabId>,
    },
    /// The answer to `asker`'s `Ask` number `request`. Addressed, because
    /// every tab hears every answer and two tabs' request numbers can be
    /// the same.
    Answer {
        request: u64,
        asker: TabId,
        outcome: AskOutcome,
    },
    /// A tab just arrived: every tab, say what you hold.
    Who,
}

/// How an asked tab answered.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AskOutcome {
    /// It let go: the port is closed and the lock released.
    Released,
    /// It did not.
    Refused(AskRefusal),
}

/// Why an asked tab did not let go.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AskRefusal {
    /// It is working on the board (a flash, an update, a push); the label is
    /// the activity's own.
    Busy(String),
    /// It does not hold the board (any more).
    NotHeld,
}

impl HoldNote {
    /// The note as channel text, said by `from`.
    pub fn encode(&self, from: &TabId) -> String {
        serde_json::to_string(&Envelope {
            v: HOLD_PROTO_VERSION,
            tab: from.clone(),
            note: self.clone(),
        })
        .expect("a hold note always serializes")
    }

    /// The tab that said it and the note, from channel text heard by
    /// `own_tab` — or `None` for a note of another version, malformed text,
    /// or this tab's own echo.
    pub fn decode(text: &str, own_tab: &TabId) -> Option<(TabId, Self)> {
        let version: VersionProbe = serde_json::from_str(text).ok()?;
        if version.v != HOLD_PROTO_VERSION {
            return None;
        }
        let envelope: Envelope = serde_json::from_str(text).ok()?;
        if &envelope.tab == own_tab {
            return None;
        }
        Some((envelope.tab, envelope.note))
    }
}

/// The note's text form: the version, the sender, and the note.
#[derive(Deserialize, Serialize)]
struct Envelope {
    v: u32,
    tab: TabId,
    note: HoldNote,
}

/// Just the version, read before anything else: a note of another version
/// may have any other shape.
#[derive(Deserialize)]
struct VersionProbe {
    v: u32,
}

#[cfg(test)]
mod tests {
    use lpa_devices::BoardKey;

    use super::*;
    use crate::app::devices::board_hold::hold_key::UsbPair;

    #[test]
    fn every_note_round_trips_through_its_text() {
        for note in notes() {
            let text = note.encode(&tab("a"));
            assert_eq!(
                HoldNote::decode(&text, &tab("b")),
                Some((tab("a"), note.clone())),
                "{text}"
            );
        }
    }

    #[test]
    fn the_text_is_the_documented_envelope() {
        let text = HoldNote::Holds {
            key: key(),
            level: HoldLevel::Watching,
            locked: true,
        }
        .encode(&tab("a"));
        assert_eq!(
            text,
            r#"{"v":1,"tab":"a","note":{"holds":{"key":"lp-board:usb:303a:1001:a0f26287b48c","level":"Watching","locked":true}}}"#
        );
        assert_eq!(
            HoldNote::Who.encode(&tab("a")),
            r#"{"v":1,"tab":"a","note":"who"}"#
        );
    }

    #[test]
    fn a_tab_never_hears_its_own_note() {
        let text = HoldNote::Who.encode(&tab("a"));
        assert_eq!(HoldNote::decode(&text, &tab("a")), None);
    }

    #[test]
    fn another_version_is_ignored_whatever_its_shape() {
        for text in [
            r#"{"v":2,"tab":"a","note":"who"}"#,
            r#"{"v":0,"tab":"a","note":"who"}"#,
            r#"{"v":2,"tab":"a","kind":"something new","payload":[1,2,3]}"#,
        ] {
            assert_eq!(HoldNote::decode(text, &tab("b")), None, "{text}");
        }
    }

    #[test]
    fn malformed_text_is_ignored() {
        for text in [
            "",
            "not json",
            "{}",
            r#"{"v":1}"#,
            r#"{"v":1,"tab":"a"}"#,
            r#"{"v":1,"tab":"a","note":"shout"}"#,
            r#"{"v":1,"tab":"a","note":{"holds":{"key":"lp-catalog","level":"Watching","locked":true}}}"#,
            r#"{"v":1,"tab":"a","note":{"gone":{}}}"#,
            r#"{"v":"1","tab":"a","note":"who"}"#,
        ] {
            assert_eq!(HoldNote::decode(text, &tab("b")), None, "{text}");
        }
    }

    #[test]
    fn an_ask_with_no_holder_named_reads_as_asking_whoever_holds_it() {
        let text =
            r#"{"v":1,"tab":"a","note":{"ask":{"request":7,"key":"lp-board:net:a0f26287b48c"}}}"#;
        assert_eq!(
            HoldNote::decode(text, &tab("b")),
            Some((
                tab("a"),
                HoldNote::Ask {
                    request: 7,
                    key: HoldKey::network(mac()),
                    holder: None,
                }
            ))
        );
    }

    fn notes() -> Vec<HoldNote> {
        vec![
            HoldNote::Holds {
                key: key(),
                level: HoldLevel::Watching,
                locked: true,
            },
            HoldNote::Holds {
                key: HoldKey::network(mac()),
                level: HoldLevel::Busy("Updating · 42%".to_string()),
                locked: false,
            },
            HoldNote::Gone { key: key() },
            HoldNote::Ask {
                request: 3,
                key: key(),
                holder: Some(tab("c")),
            },
            HoldNote::Answer {
                request: 3,
                asker: tab("b"),
                outcome: AskOutcome::Released,
            },
            HoldNote::Answer {
                request: 4,
                asker: tab("b"),
                outcome: AskOutcome::Refused(AskRefusal::Busy("Flashing · 10%".to_string())),
            },
            HoldNote::Answer {
                request: u64::MAX,
                asker: tab("b"),
                outcome: AskOutcome::Refused(AskRefusal::NotHeld),
            },
            HoldNote::Who,
        ]
    }

    fn tab(id: &str) -> TabId {
        TabId::new(id)
    }

    fn mac() -> BoardKey {
        BoardKey::parse("a0f26287b48c").expect("a mac")
    }

    fn key() -> HoldKey {
        HoldKey::usb(
            mac(),
            UsbPair {
                vendor: 0x303a,
                product: 0x1001,
            },
        )
    }
}

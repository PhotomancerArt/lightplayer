//! The **document history** projection: `ProjectHistory` → the rows the
//! project popover's History tab renders (relationship-control D10).
//!
//! The open project's history is already replayed in memory on the active
//! [`PackageHandle`](crate::app::library::PackageHandle), so this rides the
//! ordinary editor-view build rather than a fetch: capping at
//! [`HISTORY_ROW_CAP`] rows makes the projection cheaper than the
//! `project.json` read the same build already does.
//!
//! **What a row is.** One head-advancing or push event, newest first, in
//! LOG order reversed — not sorted by `at`. The log is the truth about
//! sequence; timestamps are caller-supplied and a clock that stepped
//! backwards must not reorder a document's history.
//!
//! **What is left out.** `Connected` events. A device observation is
//! *roster* evidence — "this board was carrying that version when we last
//! talked" — not something that happened to the document, and the
//! History tab answers the second question only. Nothing else is dropped.
//!
//! **What it cannot say.** A push names a device by uid, and resolving a
//! device NAME needs the device registry, which lives behind an async
//! catalog snapshot the synchronous view build cannot take. Rows say
//! `→ dev7g2k…` rather than inventing a name; same for a fork's parent
//! project. Restoring a version is not offered at all (vision D6 —
//! parked): this projection is read-only by construction.

use lpc_history::{EventKind, HistoryEvent, PrefixedUid, ProjectHistory};

/// How many recent rows the projection carries. The origin row rides on
/// top of this cap (it is the one row that explains where the document
/// came from, so it is never the row that falls off the end); there is no
/// pagination UI, so a longer history is simply not all shown.
pub const HISTORY_ROW_CAP: usize = 30;

/// What a history row IS — the small-caps word the tab prints, and the
/// only classification the UI makes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiHistoryKind {
    /// Where the document came from: the one origin event.
    Origin,
    /// A save advanced the line.
    Saved,
    /// A line version was pushed to a device.
    Pushed,
    /// A divergence was resolved by keeping one side (a clobber join).
    Joined,
}

impl UiHistoryKind {
    /// The row's kind word, lowercase — the tab renders it small-caps.
    pub fn word(self) -> &'static str {
        match self {
            UiHistoryKind::Origin => "origin",
            UiHistoryKind::Saved => "saved",
            UiHistoryKind::Pushed => "pushed",
            UiHistoryKind::Joined => "joined",
        }
    }
}

/// One row of the History tab: `vN · KIND · what · when`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiProjectHistoryEntry {
    /// The line version number this row is about, when it has one. `None`
    /// for an origin that seeds no version (`Created`, an import, an
    /// adoption): those documents reach v1 at their first save, which is
    /// its own row.
    pub version: Option<u64>,
    pub kind: UiHistoryKind,
    /// The row's "what" column — empty for a save, which says everything
    /// in its version and its kind.
    pub label: String,
    /// Wall-clock time, f64 epoch seconds (the event's own).
    pub at: f64,
}

/// The whole projection: the capped rows plus the number the next save
/// would reach.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiProjectHistory {
    /// Newest first, origin last. Empty when no history backs the open
    /// project at all (the storeless demo path).
    pub entries: Vec<UiProjectHistoryEntry>,
    /// What the next save banks — the History tab's synthetic "editing"
    /// row wears it. `None` when there is no history to advance.
    ///
    /// Best effort: re-saving older content (a revert) re-uses that
    /// content's ORIGINAL number, so a project that has reverted can bank
    /// a smaller number than this. The row is a label on work in flight,
    /// not a promise.
    pub next_version: Option<u64>,
}

impl UiProjectHistory {
    /// Project one replayed history into rows.
    pub fn from_history(history: &ProjectHistory) -> Self {
        let head_version = history
            .head()
            .and_then(|head| history.version_number(head))
            .unwrap_or(0);
        let mut entries: Vec<UiProjectHistoryEntry> = history
            .events()
            .iter()
            .rev()
            .filter_map(|event| entry(history, event))
            .collect();
        // The origin is events[0] (the type enforces it) and every origin
        // kind maps to a row, so the LAST reversed entry is the origin —
        // pop it, cut the window, put it back.
        if entries.len() > HISTORY_ROW_CAP {
            let origin = entries.pop();
            entries.truncate(HISTORY_ROW_CAP);
            entries.extend(origin);
        }
        Self {
            entries,
            next_version: Some(head_version as u64 + 1),
        }
    }
}

/// One event's row, or `None` for an event that is not document history.
fn entry(history: &ProjectHistory, event: &HistoryEvent) -> Option<UiProjectHistoryEntry> {
    let version = |hash| history.version_number(hash).map(|number| number as u64);
    let (kind, version, label) = match &event.kind {
        EventKind::Created => (UiHistoryKind::Origin, None, "created".to_string()),
        EventKind::ImportedZip => (
            UiHistoryKind::Origin,
            None,
            "imported from a .zip".to_string(),
        ),
        EventKind::ImportedJson => (
            UiHistoryKind::Origin,
            None,
            "imported from a shared envelope".to_string(),
        ),
        EventKind::RemixedFrom {
            source,
            source_version,
        } => (
            UiHistoryKind::Origin,
            source_version.and_then(version),
            format!("remixed from {source}"),
        ),
        EventKind::ForkedFrom {
            parent_project,
            parent_version,
        } => (
            UiHistoryKind::Origin,
            version(*parent_version),
            format!("forked from {}", short_uid(parent_project)),
        ),
        EventKind::PulledFromDevice { device } => (
            UiHistoryKind::Origin,
            None,
            format!("adopted from {}", short_uid(device)),
        ),
        EventKind::Saved { version: saved } => {
            (UiHistoryKind::Saved, version(*saved), String::new())
        }
        EventKind::Pushed {
            version: pushed,
            device,
            ..
        } => (
            UiHistoryKind::Pushed,
            version(*pushed),
            format!("\u{2192} {}", short_uid(device)),
        ),
        EventKind::Joined { kept, .. } => (
            UiHistoryKind::Joined,
            version(*kept),
            "kept this version \u{2014} the other was set aside".to_string(),
        ),
        // Roster evidence, not document history (see the module docs).
        EventKind::Connected { .. } => return None,
    };
    Some(UiProjectHistoryEntry {
        version,
        kind,
        label,
        at: event.at,
    })
}

/// A uid short enough for a 320px popover row: the prefix plus four body
/// characters. Enough to tell two boards apart, and honest about being a
/// uid rather than a name.
fn short_uid(uid: &PrefixedUid) -> String {
    format!("{}{}\u{2026}", uid.prefix(), &uid.body_str()[..4])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{ContentHash, UidPrefix};

    fn hash(data: &[u8]) -> ContentHash {
        ContentHash::of(data)
    }

    fn device() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Device, &[1u8; 16])
    }

    fn created() -> ProjectHistory {
        ProjectHistory::new(HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        })
        .expect("origin")
    }

    /// Every `EventKind` the log can carry lands somewhere: eight kinds
    /// become rows with the right kind word and version, and `Connected`
    /// is deliberately absent.
    #[test]
    fn every_event_kind_maps_to_its_row() {
        let mut history = created();
        history.record_save(hash(b"v1"), 2.0);
        history
            .record_push(hash(b"v1"), device(), 3.0, None)
            .unwrap();
        history.record_connect(device(), hash(b"foreign"), 4.0);
        history
            .record_join(hash(b"foreign"), hash(b"v1"), 5.0)
            .unwrap();

        let view = UiProjectHistory::from_history(&history);
        let rows: Vec<_> = view
            .entries
            .iter()
            .map(|entry| (entry.kind, entry.version, entry.label.as_str(), entry.at))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    UiHistoryKind::Joined,
                    Some(2),
                    "kept this version \u{2014} the other was set aside",
                    5.0
                ),
                (
                    UiHistoryKind::Pushed,
                    Some(1),
                    "\u{2192} dev040g\u{2026}",
                    3.0
                ),
                (UiHistoryKind::Saved, Some(1), "", 2.0),
                (UiHistoryKind::Origin, None, "created", 1.0),
            ],
            "a Connected event is roster evidence, not a history row"
        );
    }

    /// The four other origin kinds and their wording. Each is the only
    /// row a fresh history has, and only the two that seed a version
    /// carry one.
    #[test]
    fn every_origin_kind_names_where_the_document_came_from() {
        let origins = [
            (EventKind::ImportedZip, None, "imported from a .zip"),
            (
                EventKind::ImportedJson,
                None,
                "imported from a shared envelope",
            ),
            (
                EventKind::PulledFromDevice { device: device() },
                None,
                "adopted from dev040g\u{2026}",
            ),
            (
                EventKind::RemixedFrom {
                    source: "examples/small-dome".to_string(),
                    source_version: Some(hash(b"seed")),
                },
                Some(1),
                "remixed from examples/small-dome",
            ),
            (
                EventKind::ForkedFrom {
                    parent_project: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
                    parent_version: hash(b"seed"),
                },
                Some(1),
                "forked from prj0810\u{2026}",
            ),
        ];
        for (kind, version, label) in origins {
            let history =
                ProjectHistory::new(HistoryEvent { at: 7.0, kind }).expect("origin history");
            let view = UiProjectHistory::from_history(&history);
            let [entry] = &view.entries[..] else {
                panic!("an origin-only history is exactly one row");
            };
            assert_eq!(entry.kind, UiHistoryKind::Origin);
            assert_eq!(entry.version, version, "{label}");
            assert_eq!(entry.label, label);
        }
    }

    /// Newest first, and by LOG order — a timestamp that went backwards
    /// does not reshuffle the document's sequence.
    #[test]
    fn rows_are_newest_first_in_log_order() {
        let mut history = created();
        history.record_save(hash(b"v1"), 100.0);
        // a clock that stepped backwards between saves
        history.record_save(hash(b"v2"), 50.0);

        let view = UiProjectHistory::from_history(&history);
        assert_eq!(
            view.entries
                .iter()
                .map(|entry| entry.version)
                .collect::<Vec<_>>(),
            vec![Some(2), Some(1), None]
        );
    }

    /// The cap keeps the recent window AND the origin: a document with a
    /// long history still says where it came from.
    #[test]
    fn the_cap_keeps_the_recent_window_and_the_origin() {
        let mut history = created();
        for n in 0..50u32 {
            history.record_save(hash(&n.to_be_bytes()), 10.0 + f64::from(n));
        }

        let view = UiProjectHistory::from_history(&history);
        assert_eq!(view.entries.len(), HISTORY_ROW_CAP + 1);
        // newest save first…
        assert_eq!(view.entries[0].version, Some(50));
        assert_eq!(view.entries[0].kind, UiHistoryKind::Saved);
        // …the window's oldest save…
        assert_eq!(view.entries[HISTORY_ROW_CAP - 1].version, Some(21));
        // …and the origin, always.
        let origin = view.entries.last().expect("rows");
        assert_eq!(origin.kind, UiHistoryKind::Origin);
        assert_eq!(origin.at, 1.0);
    }

    /// A history shorter than the cap is not padded and keeps its origin
    /// exactly once.
    #[test]
    fn a_short_history_is_untruncated() {
        let mut history = created();
        history.record_save(hash(b"v1"), 2.0);
        let view = UiProjectHistory::from_history(&history);
        assert_eq!(view.entries.len(), 2);
        assert_eq!(
            view.entries
                .iter()
                .filter(|entry| entry.kind == UiHistoryKind::Origin)
                .count(),
            1
        );
    }

    /// The next-save number follows the head — v1 for a document that has
    /// never saved, head + 1 after that.
    #[test]
    fn next_version_follows_the_head() {
        let mut history = created();
        assert_eq!(
            UiProjectHistory::from_history(&history).next_version,
            Some(1)
        );
        history.record_save(hash(b"v1"), 2.0);
        history.record_save(hash(b"v2"), 3.0);
        assert_eq!(
            UiProjectHistory::from_history(&history).next_version,
            Some(3)
        );
    }

    /// No history at all (the storeless demo path) is empty and claims no
    /// next version — the tab says so rather than painting a v1 row.
    #[test]
    fn the_default_projection_claims_nothing() {
        let empty = UiProjectHistory::default();
        assert!(empty.entries.is_empty());
        assert_eq!(empty.next_version, None);
    }
}

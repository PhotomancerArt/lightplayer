//! The replayed, in-memory view of one project's history line.
//!
//! A project's history is **linear**: an origin event, then saves advancing
//! the head. Forks mint a new project uid whose history begins with a
//! `ForkedFrom` origin — there is no DAG. Two membership notions exist:
//!
//! - the **line** (origin version + saves) — what `contains` and `classify`
//!   consult; this is the project's own version sequence.
//! - **known versions** (line + versions observed via `Connected` events) —
//!   what fork parents may reference; a diverged device copy is snapshotted
//!   at connect and may be forked from, even though it was never saved to
//!   this line.

use alloc::vec::Vec;

use crate::event::event_log::EventLog;
use crate::event::geo_point::GeoPoint;
use crate::event::history_event::{EventKind, HistoryEvent};
use crate::hash::content_hash::ContentHash;
use crate::history_error::HistoryError;
use crate::lineage::sync_relation::SyncRelation;
use crate::uid::prefixed_uid::PrefixedUid;

/// Replayed view over a project's events.
///
/// Recording methods mutate this view and return the event so callers can
/// append it to the persistent [`EventLog`]; this type never does IO itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectHistory {
    events: Vec<HistoryEvent>,
    /// The version line: origin version (if the origin has one) + saves.
    line: Vec<ContentHash>,
    /// Versions observed via `Connected` that are not in the line.
    connected: Vec<ContentHash>,
}

impl ProjectHistory {
    /// Start a history with an origin event.
    pub fn new(origin: HistoryEvent) -> Result<Self, HistoryError> {
        if !origin.kind.is_origin() {
            return Err(HistoryError::InvalidHistory(
                "history must start with an origin event",
            ));
        }
        let mut line = Vec::new();
        if let Some(version) = origin.kind.origin_version() {
            line.push(version);
        }
        Ok(Self {
            events: alloc::vec![origin],
            line,
            connected: Vec::new(),
        })
    }

    /// Replay a history from its full event sequence.
    pub fn from_events(events: Vec<HistoryEvent>) -> Result<Self, HistoryError> {
        let mut iter = events.into_iter();
        let origin = iter.next().ok_or(HistoryError::InvalidHistory(
            "history must start with an origin event",
        ))?;
        let mut history = Self::new(origin)?;
        for event in iter {
            history.replay(event)?;
        }
        Ok(history)
    }

    /// Load and replay a history from a persistent log.
    pub fn load(log: &EventLog<'_>) -> Result<Self, HistoryError> {
        Self::from_events(log.read_all()?)
    }

    fn replay(&mut self, event: HistoryEvent) -> Result<(), HistoryError> {
        match &event.kind {
            kind if kind.is_origin() => {
                return Err(HistoryError::InvalidHistory("multiple origin events"));
            }
            EventKind::Saved { version } => self.line.push(*version),
            EventKind::Pushed { version, .. } => {
                if !self.contains(*version) {
                    return Err(HistoryError::InvalidHistory(
                        "push of a version not in the line",
                    ));
                }
            }
            EventKind::Connected { observed, .. } => {
                if !self.knows(*observed) {
                    self.connected.push(*observed);
                }
            }
            _ => {}
        }
        self.events.push(event);
        Ok(())
    }

    pub fn events(&self) -> &[HistoryEvent] {
        &self.events
    }

    /// The current head of the line, if any version exists yet.
    pub fn head(&self) -> Option<ContentHash> {
        self.line.last().copied()
    }

    /// Whether a version is in the line (origin version or any save).
    pub fn contains(&self, version: ContentHash) -> bool {
        self.line.contains(&version)
    }

    /// Whether a version is in the line or was observed via a connect.
    pub fn knows(&self, version: ContentHash) -> bool {
        self.contains(version) || self.connected.contains(&version)
    }

    /// Relate an observed version (e.g. a device's copy) to this line.
    pub fn classify(&self, observed: ContentHash) -> SyncRelation {
        if self.head() == Some(observed) {
            SyncRelation::AtHead
        } else if self.contains(observed) {
            SyncRelation::Behind
        } else {
            SyncRelation::Diverged
        }
    }

    /// 1-based version number of the *first* occurrence in the line.
    ///
    /// The line is a save sequence: re-saving old content (a revert) appends
    /// the same hash again; this returns the first occurrence. Display
    /// concerns beyond that are the UI's problem.
    pub fn version_number(&self, version: ContentHash) -> Option<usize> {
        self.line.iter().position(|v| *v == version).map(|i| i + 1)
    }

    /// Record a save advancing the head. Returns the event for logging.
    pub fn record_save(&mut self, version: ContentHash, at: f64) -> HistoryEvent {
        let event = HistoryEvent {
            at,
            kind: EventKind::Saved { version },
        };
        self.line.push(version);
        self.events.push(event.clone());
        event
    }

    /// Record a push of a line version to a device.
    pub fn record_push(
        &mut self,
        version: ContentHash,
        device: PrefixedUid,
        at: f64,
        location: Option<GeoPoint>,
    ) -> Result<HistoryEvent, HistoryError> {
        if !self.contains(version) {
            return Err(HistoryError::UnknownVersion(version));
        }
        let event = HistoryEvent {
            at,
            kind: EventKind::Pushed {
                version,
                device,
                location,
            },
        };
        self.events.push(event.clone());
        Ok(event)
    }

    /// Record a device observation (connect-as-pull bookkeeping).
    pub fn record_connect(
        &mut self,
        device: PrefixedUid,
        observed: ContentHash,
        at: f64,
    ) -> HistoryEvent {
        let event = HistoryEvent {
            at,
            kind: EventKind::Connected { device, observed },
        };
        if !self.knows(observed) {
            self.connected.push(observed);
        }
        self.events.push(event.clone());
        event
    }

    /// Start a new line forked from a version this history knows.
    ///
    /// `parent_project` is the parent's uid (histories do not store their own
    /// uid — the log lives in the project's history area). Known-but-unsaved
    /// versions (diverged device copies observed at connect) are valid fork
    /// parents.
    pub fn fork_from(
        parent: &ProjectHistory,
        parent_project: PrefixedUid,
        parent_version: ContentHash,
        at: f64,
    ) -> Result<ProjectHistory, HistoryError> {
        if !parent.knows(parent_version) {
            return Err(HistoryError::UnknownVersion(parent_version));
        }
        Self::new(HistoryEvent {
            at,
            kind: EventKind::ForkedFrom {
                parent_project,
                parent_version,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uid::uid_prefix::UidPrefix;

    fn hash(data: &[u8]) -> ContentHash {
        ContentHash::of(data)
    }

    fn dev() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Device, &[1u8; 16])
    }

    fn created() -> HistoryEvent {
        HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        }
    }

    #[test]
    fn empty_history_is_invalid() {
        assert!(matches!(
            ProjectHistory::from_events(alloc::vec![]),
            Err(HistoryError::InvalidHistory(_))
        ));
    }

    #[test]
    fn history_must_start_with_origin() {
        let save = HistoryEvent {
            at: 1.0,
            kind: EventKind::Saved {
                version: hash(b"a"),
            },
        };
        assert!(ProjectHistory::new(save.clone()).is_err());
        assert!(ProjectHistory::from_events(alloc::vec![save]).is_err());
    }

    #[test]
    fn duplicate_origin_rejected() {
        assert!(matches!(
            ProjectHistory::from_events(alloc::vec![created(), created()]),
            Err(HistoryError::InvalidHistory("multiple origin events"))
        ));
    }

    #[test]
    fn origin_without_version_has_no_head() {
        let history = ProjectHistory::new(created()).unwrap();
        assert_eq!(history.head(), None);
        assert_eq!(history.classify(hash(b"x")), SyncRelation::Diverged);
    }

    #[test]
    fn fork_origin_version_is_v1_head() {
        let parent_uid = PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]);
        let origin = HistoryEvent {
            at: 1.0,
            kind: EventKind::ForkedFrom {
                parent_project: parent_uid,
                parent_version: hash(b"a"),
            },
        };
        let history = ProjectHistory::new(origin).unwrap();
        assert_eq!(history.head(), Some(hash(b"a")));
        assert_eq!(history.version_number(hash(b"a")), Some(1));
        assert_eq!(history.classify(hash(b"a")), SyncRelation::AtHead);
    }

    #[test]
    fn classify_matrix() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        history.record_save(hash(b"v2"), 3.0);
        assert_eq!(history.classify(hash(b"v2")), SyncRelation::AtHead);
        assert_eq!(history.classify(hash(b"v1")), SyncRelation::Behind);
        assert_eq!(history.classify(hash(b"other")), SyncRelation::Diverged);
    }

    #[test]
    fn push_requires_a_line_version() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        assert!(history.record_push(hash(b"v1"), dev(), 3.0, None).is_ok());
        assert!(matches!(
            history.record_push(hash(b"nope"), dev(), 4.0, None),
            Err(HistoryError::UnknownVersion(_))
        ));
    }

    #[test]
    fn connect_observations_are_known_but_not_in_line() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        history.record_connect(dev(), hash(b"foreign"), 3.0);
        assert!(history.knows(hash(b"foreign")));
        assert!(!history.contains(hash(b"foreign")));
        // still diverged on reconnect: observation does not join the line
        assert_eq!(history.classify(hash(b"foreign")), SyncRelation::Diverged);
    }

    #[test]
    fn fork_validates_parent_version() {
        let mut parent = ProjectHistory::new(created()).unwrap();
        parent.record_save(hash(b"v1"), 2.0);
        parent.record_connect(dev(), hash(b"foreign"), 3.0);
        let parent_uid = PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]);

        // line version: ok
        assert!(ProjectHistory::fork_from(&parent, parent_uid, hash(b"v1"), 4.0).is_ok());
        // connect-observed version: ok (diverged device copy)
        assert!(ProjectHistory::fork_from(&parent, parent_uid, hash(b"foreign"), 5.0).is_ok());
        // unknown: rejected
        assert!(matches!(
            ProjectHistory::fork_from(&parent, parent_uid, hash(b"nope"), 6.0),
            Err(HistoryError::UnknownVersion(_))
        ));
    }

    #[test]
    fn revert_re_saves_old_hash_and_keeps_first_version_number() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        history.record_save(hash(b"v2"), 3.0);
        history.record_save(hash(b"v1"), 4.0);
        assert_eq!(history.head(), Some(hash(b"v1")));
        assert_eq!(history.version_number(hash(b"v1")), Some(1));
        assert_eq!(history.classify(hash(b"v1")), SyncRelation::AtHead);
        assert_eq!(history.classify(hash(b"v2")), SyncRelation::Behind);
    }

    #[test]
    fn replay_round_trip() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        history.record_push(hash(b"v1"), dev(), 3.0, None).unwrap();
        history.record_connect(dev(), hash(b"foreign"), 4.0);

        let replayed = ProjectHistory::from_events(history.events().to_vec()).unwrap();
        assert_eq!(replayed, history);
    }
}

//! The replayed, in-memory view of one project's history.
//!
//! A project's history is an **append-only ordered event log whose ancestry
//! forms a DAG**: an origin event, then head-advancing events — saves, and
//! **clobber joins** (`Joined`), the DAG's only non-linear node: two
//! parents, content equal to the chosen one. There is no computed content
//! merge (yet — a future merge is a join whose content is derived rather
//! than picked). Forks still mint a new project uid whose history begins
//! with a `ForkedFrom` origin. Three membership notions exist:
//!
//! - the **line** (origin version + saves + joins' kept versions) — the
//!   project's own head-advancing sequence; what `contains` consults.
//! - **superseded versions** — the `set_aside` side of joins: chosen past,
//!   never destroyed. `classify` reports them [`SyncRelation::Behind`], so a
//!   peer still carrying a losing version fast-forwards instead of
//!   re-diverging.
//! - **known versions** (line + superseded + versions observed via
//!   `Connected` events) — what fork parents may reference; a diverged
//!   device copy is snapshotted at connect and may be forked from, even
//!   though it was never saved to this line.

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
    /// The head-advancing sequence: origin version (if the origin has one),
    /// saves, and joins' `kept` versions.
    line: Vec<ContentHash>,
    /// Versions set aside by joins — chosen past, still reachable.
    superseded: Vec<ContentHash>,
    /// Versions observed via `Connected` that are not otherwise known.
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
            superseded: Vec::new(),
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
            EventKind::Joined { kept, set_aside } => {
                self.check_join(*kept, *set_aside)?;
                self.apply_join(*kept, *set_aside);
            }
            _ => {}
        }
        self.events.push(event);
        Ok(())
    }

    /// A join resolves a divergence between this history's head and one
    /// foreign version: exactly one side must be the current head.
    fn check_join(&self, kept: ContentHash, set_aside: ContentHash) -> Result<(), HistoryError> {
        if kept == set_aside {
            return Err(HistoryError::InvalidHistory(
                "join must choose between two distinct versions",
            ));
        }
        let Some(head) = self.head() else {
            return Err(HistoryError::InvalidHistory("join requires a current head"));
        };
        if (kept == head) == (set_aside == head) {
            return Err(HistoryError::InvalidHistory(
                "exactly one side of a join must be the current head",
            ));
        }
        Ok(())
    }

    fn apply_join(&mut self, kept: ContentHash, set_aside: ContentHash) {
        self.line.push(kept);
        if !self.knows(set_aside) {
            self.superseded.push(set_aside);
        }
    }

    pub fn events(&self) -> &[HistoryEvent] {
        &self.events
    }

    /// The current head of the line, if any version exists yet.
    pub fn head(&self) -> Option<ContentHash> {
        self.line.last().copied()
    }

    /// Whether a version is in the line (origin version, any save, or a
    /// join's kept version).
    pub fn contains(&self, version: ContentHash) -> bool {
        self.line.contains(&version)
    }

    /// Whether a version was set aside by a join (chosen past).
    pub fn superseded(&self, version: ContentHash) -> bool {
        self.superseded.contains(&version)
    }

    /// Whether a version is in the line, was set aside by a join, or was
    /// observed via a connect.
    pub fn knows(&self, version: ContentHash) -> bool {
        self.contains(version)
            || self.superseded.contains(&version)
            || self.connected.contains(&version)
    }

    /// Relate an observed version (e.g. a device's or peer's copy) to this
    /// history. Superseded versions read as [`SyncRelation::Behind`]: a
    /// peer still carrying a join's losing side fast-forwards instead of
    /// re-diverging.
    pub fn classify(&self, observed: ContentHash) -> SyncRelation {
        if self.head() == Some(observed) {
            SyncRelation::AtHead
        } else if self.contains(observed) || self.superseded.contains(&observed) {
            SyncRelation::Behind
        } else {
            SyncRelation::Diverged
        }
    }

    /// When a line version last became current (f64 epoch seconds): the
    /// newest `Saved` or `Joined { kept }` event carrying it, falling back
    /// to the origin event when the version is the origin's. `None` for
    /// versions not in the line.
    pub fn saved_at(&self, version: ContentHash) -> Option<f64> {
        self.events
            .iter()
            .rev()
            .find_map(|event| match &event.kind {
                EventKind::Saved { version: saved } if *saved == version => Some(event.at),
                EventKind::Joined { kept, .. } if *kept == version => Some(event.at),
                kind if kind.is_origin() && kind.origin_version() == Some(version) => {
                    Some(event.at)
                }
                _ => None,
            })
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

    /// Record a clobber join resolving a divergence (D7): the head after
    /// the join is `kept`; `set_aside` stays reachable and classifies as
    /// [`SyncRelation::Behind`] thereafter.
    ///
    /// Exactly one side must be the current head. The two call shapes:
    /// - **keep mine**: `kept` = local head, `set_aside` = the observed
    ///   foreign version.
    /// - **use theirs**: `kept` = the observed foreign version,
    ///   `set_aside` = local head. The caller must have banked the foreign
    ///   snapshot *before* recording the join — the history only tracks
    ///   hashes; content survival is the snapshot store's job.
    pub fn record_join(
        &mut self,
        kept: ContentHash,
        set_aside: ContentHash,
        at: f64,
    ) -> Result<HistoryEvent, HistoryError> {
        self.check_join(kept, set_aside)?;
        self.apply_join(kept, set_aside);
        let event = HistoryEvent {
            at,
            kind: EventKind::Joined { kept, set_aside },
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
    fn join_keep_mine() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"mine"), 2.0);
        let event = history
            .record_join(hash(b"mine"), hash(b"theirs"), 3.0)
            .unwrap();
        assert!(matches!(event.kind, EventKind::Joined { .. }));
        assert_eq!(history.head(), Some(hash(b"mine")));
        // the losing side is Behind, not Diverged — peers fast-forward
        assert_eq!(history.classify(hash(b"theirs")), SyncRelation::Behind);
        assert!(history.knows(hash(b"theirs")));
        assert!(!history.contains(hash(b"theirs")));
        assert!(history.superseded(hash(b"theirs")));
    }

    #[test]
    fn join_use_theirs() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"mine"), 2.0);
        history
            .record_join(hash(b"theirs"), hash(b"mine"), 3.0)
            .unwrap();
        assert_eq!(history.head(), Some(hash(b"theirs")));
        // old local head stays reachable via the line
        assert_eq!(history.classify(hash(b"mine")), SyncRelation::Behind);
        assert!(history.contains(hash(b"theirs")));
        // joins are head-advancing: kept gets a version number and a time
        assert_eq!(history.version_number(hash(b"theirs")), Some(2));
        assert_eq!(history.saved_at(hash(b"theirs")), Some(3.0));
    }

    #[test]
    fn join_preconditions() {
        // no head yet
        let mut empty = ProjectHistory::new(created()).unwrap();
        assert!(matches!(
            empty.record_join(hash(b"a"), hash(b"b"), 2.0),
            Err(HistoryError::InvalidHistory("join requires a current head"))
        ));

        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"mine"), 2.0);
        // kept == set_aside
        assert!(history.record_join(hash(b"x"), hash(b"x"), 3.0).is_err());
        // neither side is the head
        assert!(matches!(
            history.record_join(hash(b"a"), hash(b"b"), 3.0),
            Err(HistoryError::InvalidHistory(
                "exactly one side of a join must be the current head"
            ))
        ));
        // both sides claim the head is impossible (kept == set_aside covers it)
        // failed attempts must not have mutated anything
        assert_eq!(history.head(), Some(hash(b"mine")));
        assert_eq!(history.events().len(), 2);
        assert!(!history.knows(hash(b"a")));
    }

    #[test]
    fn join_after_join_continues_the_line() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        history
            .record_join(hash(b"remote1"), hash(b"v1"), 3.0)
            .unwrap();
        history.record_save(hash(b"v2"), 4.0);
        history
            .record_join(hash(b"v2"), hash(b"remote2"), 5.0)
            .unwrap();
        assert_eq!(history.head(), Some(hash(b"v2")));
        for known in [b"v1" as &[u8], b"remote1", b"remote2"] {
            assert_eq!(history.classify(hash(known)), SyncRelation::Behind);
        }
        assert_eq!(history.classify(hash(b"other")), SyncRelation::Diverged);
    }

    #[test]
    fn join_replay_round_trip() {
        let mut history = ProjectHistory::new(created()).unwrap();
        history.record_save(hash(b"mine"), 2.0);
        history
            .record_join(hash(b"theirs"), hash(b"mine"), 3.0)
            .unwrap();
        history.record_save(hash(b"v3"), 4.0);
        let replayed = ProjectHistory::from_events(history.events().to_vec()).unwrap();
        assert_eq!(replayed, history);
    }

    #[test]
    fn replay_rejects_invalid_join() {
        // a Joined event whose sides don't straddle the head must not replay
        let events = alloc::vec![
            created(),
            HistoryEvent {
                at: 2.0,
                kind: EventKind::Saved {
                    version: hash(b"v1")
                },
            },
            HistoryEvent {
                at: 3.0,
                kind: EventKind::Joined {
                    kept: hash(b"a"),
                    set_aside: hash(b"b"),
                },
            },
        ];
        assert!(ProjectHistory::from_events(events).is_err());
    }

    #[test]
    fn superseded_version_is_valid_fork_parent() {
        let mut parent = ProjectHistory::new(created()).unwrap();
        parent.record_save(hash(b"mine"), 2.0);
        parent
            .record_join(hash(b"mine"), hash(b"theirs"), 3.0)
            .unwrap();
        let parent_uid = PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]);
        assert!(ProjectHistory::fork_from(&parent, parent_uid, hash(b"theirs"), 4.0).is_ok());
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

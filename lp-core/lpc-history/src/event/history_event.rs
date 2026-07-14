//! The history event vocabulary.
//!
//! Events are a persistence format (JSONL, see [`super::event_log`]); field
//! and variant names are deliberate and stable. Timestamps are f64 epoch
//! seconds, always caller-supplied — this crate never reads a clock.
//!
//! Exactly one origin event (`Created`, `ImportedZip`, `RemixedFrom`,
//! `ForkedFrom`, or `PulledFromDevice`) appears in a project's history, and
//! it is the first event — enforced by
//! [`crate::lineage::project_history::ProjectHistory`].

use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::event::geo_point::GeoPoint;
use crate::hash::content_hash::ContentHash;
use crate::uid::prefixed_uid::PrefixedUid;

/// One entry in a project's history log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEvent {
    /// Wall-clock time, f64 epoch seconds, caller-supplied.
    pub at: f64,
    pub kind: EventKind,
}

/// What happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    /// Origin: the project was created from scratch.
    Created,
    /// Origin: the project was imported from a zip archive.
    ImportedZip,
    /// Origin: the project was remixed from a package at some place
    /// (e.g. an example on the examples site).
    RemixedFrom {
        source: String,
        source_version: Option<ContentHash>,
    },
    /// Origin: the project was forked from another project's version.
    ForkedFrom {
        parent_project: PrefixedUid,
        parent_version: ContentHash,
    },
    /// Origin: adopted from a device carrying a project this library did
    /// not know (connect-as-pull, D8/D11). The adopted content itself is
    /// the first `Saved` version; the device observation follows as a
    /// `Connected` event.
    PulledFromDevice { device: PrefixedUid },
    /// A save advanced the line to this version.
    Saved { version: ContentHash },
    /// This version was pushed to a device.
    Pushed {
        version: ContentHash,
        device: PrefixedUid,
        location: Option<GeoPoint>,
    },
    /// A device was observed (at connect) carrying this version.
    Connected {
        device: PrefixedUid,
        observed: ContentHash,
    },
}

impl EventKind {
    /// Whether this kind starts a project's history.
    pub fn is_origin(&self) -> bool {
        matches!(
            self,
            EventKind::Created
                | EventKind::ImportedZip
                | EventKind::RemixedFrom { .. }
                | EventKind::ForkedFrom { .. }
                | EventKind::PulledFromDevice { .. }
        )
    }

    /// The version an origin event seeds the line with (v1), if it has one.
    pub fn origin_version(&self) -> Option<ContentHash> {
        match self {
            EventKind::RemixedFrom { source_version, .. } => *source_version,
            EventKind::ForkedFrom { parent_version, .. } => Some(*parent_version),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn uid(prefix: crate::uid::uid_prefix::UidPrefix) -> PrefixedUid {
        PrefixedUid::mint(prefix, &[0u8; 16])
    }

    #[test]
    fn every_variant_round_trips() {
        use crate::uid::uid_prefix::UidPrefix;
        let hash = ContentHash::of(b"v");
        let events = [
            EventKind::Created,
            EventKind::ImportedZip,
            EventKind::RemixedFrom {
                source: "examples:rainbow".to_string(),
                source_version: Some(hash),
            },
            EventKind::ForkedFrom {
                parent_project: uid(UidPrefix::Project),
                parent_version: hash,
            },
            EventKind::Saved { version: hash },
            EventKind::Pushed {
                version: hash,
                device: uid(UidPrefix::Device),
                location: Some(GeoPoint {
                    lat: 1.5,
                    lon: -2.5,
                    label: None,
                }),
            },
            EventKind::Connected {
                device: uid(UidPrefix::Device),
                observed: hash,
            },
            EventKind::PulledFromDevice {
                device: uid(UidPrefix::Device),
            },
        ];
        for kind in events {
            let event = HistoryEvent {
                at: 1700000000.5,
                kind,
            };
            let json = serde_json::to_string(&event).unwrap();
            let back: HistoryEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(back, event);
        }
    }

    #[test]
    fn committed_json_samples_pin_the_format() {
        // These literals are the persistence format. Changing them breaks
        // existing event logs — that must be a deliberate act.
        let saved = HistoryEvent {
            at: 1700000000.5,
            kind: EventKind::Saved {
                version: ContentHash::of(b"v"),
            },
        };
        assert_eq!(
            serde_json::to_string(&saved).unwrap(),
            alloc::format!(
                "{{\"at\":1700000000.5,\"kind\":{{\"Saved\":{{\"version\":\"{}\"}}}}}}",
                ContentHash::of(b"v")
            )
        );

        let created = HistoryEvent {
            at: 2.0,
            kind: EventKind::Created,
        };
        assert_eq!(
            serde_json::to_string(&created).unwrap(),
            "{\"at\":2.0,\"kind\":\"Created\"}"
        );
    }

    #[test]
    fn origin_classification() {
        let hash = ContentHash::of(b"v");
        assert!(EventKind::Created.is_origin());
        assert!(EventKind::ImportedZip.is_origin());
        assert!(
            EventKind::PulledFromDevice {
                device: uid(crate::uid::uid_prefix::UidPrefix::Device)
            }
            .is_origin()
        );
        assert!(!EventKind::Saved { version: hash }.is_origin());
        assert_eq!(EventKind::Created.origin_version(), None);
        assert_eq!(
            EventKind::RemixedFrom {
                source: "x".to_string(),
                source_version: Some(hash)
            }
            .origin_version(),
            Some(hash)
        );
    }
}

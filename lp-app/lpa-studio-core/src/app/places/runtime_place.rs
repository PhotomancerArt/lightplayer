//! Runtime places: the simulator (and, with M5, devices) as push/pull
//! targets.

use lpa_link::LinkConnectionKind;
use lpc_history::{ContentHash, ProjectHistory, SyncRelation};

use super::place::{Place, PlaceDescriptor, PlaceKind};

/// A connected runtime as a place. One storage slot today (the studio uses
/// a single `"studio"` storage id); real multi-slot capacity is a future
/// place capability.
pub struct RuntimePlace {
    connection_kind: LinkConnectionKind,
}

impl RuntimePlace {
    pub fn new(connection_kind: LinkConnectionKind) -> Self {
        Self { connection_kind }
    }

    pub fn is_device(&self) -> bool {
        !matches!(
            self.connection_kind,
            LinkConnectionKind::BrowserWorker { .. }
        )
    }
}

impl Place for RuntimePlace {
    fn descriptor(&self) -> PlaceDescriptor {
        let kind = if self.is_device() {
            PlaceKind::Device
        } else {
            PlaceKind::SimRuntime
        };
        PlaceDescriptor {
            kind,
            capacity: Some(1),
        }
    }
}

/// Relate a runtime's observed package hash to a library project's line.
///
/// The connect-as-pull decision surface (D11): `AtHead` → nothing to do;
/// `Behind` → offer Update (never as the default click); `Diverged` → the
/// copy was already banked at connect, offer adopt / keep-both / replace.
/// A runtime whose package has an *unknown uid* has no history to relate —
/// the caller installs a new library package with pulled provenance instead
/// of calling this.
pub fn relate_runtime_content(history: &ProjectHistory, observed: ContentHash) -> SyncRelation {
    history.classify(observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{EventKind, HistoryEvent};

    #[test]
    fn descriptor_derives_from_connection_kind() {
        let sim = RuntimePlace::new(LinkConnectionKind::BrowserWorker {
            protocol: "json".to_string(),
        });
        assert_eq!(sim.descriptor().kind, PlaceKind::SimRuntime);
        assert_eq!(sim.descriptor().capacity, Some(1));
    }

    #[test]
    fn relate_covers_the_connect_matrix() {
        let mut history = ProjectHistory::new(HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        })
        .unwrap();
        let v1 = ContentHash::of(b"v1");
        let v2 = ContentHash::of(b"v2");
        history.record_save(v1, 2.0);
        history.record_save(v2, 3.0);

        assert_eq!(relate_runtime_content(&history, v2), SyncRelation::AtHead);
        assert_eq!(relate_runtime_content(&history, v1), SyncRelation::Behind);
        assert_eq!(
            relate_runtime_content(&history, ContentHash::of(b"foreign")),
            SyncRelation::Diverged
        );
    }
}

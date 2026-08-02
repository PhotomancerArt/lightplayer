//! Panel writer store: lazy, unauthored runtime writers per
//! `(scope, channel)` (panel.md P1–P4).
//!
//! Panel controls are stateful runtime data sources, never authored
//! bindings: a writer materializes only when a control is first touched
//! (lazy — otherwise every public param would self-shadow and outer scopes
//! could never drive inner channels), holds its value until an explicit
//! clear (latch, P2), and lives on the Engine so it survives
//! `apply_project_changes` (which rebuilds bindings from defs — a panel
//! writer registered as a binding would be silently destroyed by the first
//! authoring edit; that is why this is a side store, not a
//! `BindingDraft`).
//!
//! Firmware-safe by construction: `VecMap`, `no_std` + alloc.

use lp_collection::VecMap;
use lpc_model::{ChannelName, LpValue, Revision};

use crate::node::ScopeRef;

/// One engaged panel writer.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelWriter {
    /// The held value (latched until cleared, panel.md P2).
    pub value: LpValue,
    /// Engine revision of the most recent write (display/coalescing aid).
    pub written_at: Revision,
    /// Momentary liveness deadline (panel.md P14), in engine total-ms:
    /// gesture channels write while active and DESPAWN past this instant —
    /// the despawn IS the fallback mechanism. `None` = latching writer.
    /// Renewal is simply another write. Momentary writers never persist.
    pub expires_at_ms: Option<u64>,
}

/// Engine-owned store of engaged panel writers, keyed by the scope the
/// write lands in plus the channel it drives.
#[derive(Debug, Default)]
pub struct PanelWriterStore {
    writers: VecMap<(ScopeRef, ChannelName), PanelWriter>,
}

impl PanelWriterStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Engage (or update) the writer for `(scope, channel)`.
    pub fn set(
        &mut self,
        scope: ScopeRef,
        channel: ChannelName,
        value: LpValue,
        at: Revision,
        expires_at_ms: Option<u64>,
    ) {
        self.writers.insert(
            (scope, channel),
            PanelWriter {
                value,
                written_at: at,
                expires_at_ms,
            },
        );
    }

    /// Despawn every momentary writer whose deadline has passed (a dropped
    /// client must release its gesture). Returns the number despawned.
    pub fn despawn_expired(&mut self, now_ms: u64) -> usize {
        let expired: alloc::vec::Vec<(ScopeRef, ChannelName)> = self
            .writers
            .iter()
            .filter(|(_, writer)| {
                writer
                    .expires_at_ms
                    .is_some_and(|deadline| now_ms >= deadline)
            })
            .map(|(key, _)| key.clone())
            .collect();
        let count = expired.len();
        for key in expired {
            self.writers.remove(&key);
        }
        count
    }

    /// Clear EVERYTHING — including sink-scope writers (settled P-Q4: a
    /// playlist entry's latched value clears too). Returns the count.
    pub fn clear_all(&mut self) -> usize {
        let count = self.writers.len();
        self.writers = VecMap::new();
        count
    }

    /// Clear one writer. Returns true when something was engaged.
    pub fn clear(&mut self, scope: ScopeRef, channel: &ChannelName) -> bool {
        self.writers.remove(&(scope, channel.clone())).is_some()
    }

    /// Clear every writer in `scope`. Returns the number cleared.
    pub fn clear_scope(&mut self, scope: ScopeRef) -> usize {
        let keys: alloc::vec::Vec<(ScopeRef, ChannelName)> = self
            .writers
            .iter()
            .filter(|((s, _), _)| *s == scope)
            .map(|(key, _)| key.clone())
            .collect();
        let cleared = keys.len();
        for key in keys {
            self.writers.remove(&key);
        }
        cleared
    }

    /// The engaged writer for `(scope, channel)`, if any.
    pub fn get(&self, scope: ScopeRef, channel: &ChannelName) -> Option<&PanelWriter> {
        self.writers.get(&(scope, channel.clone()))
    }

    /// Every engaged writer.
    pub fn iter(&self) -> impl Iterator<Item = (&(ScopeRef, ChannelName), &PanelWriter)> {
        self.writers.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.writers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.writers.len()
    }
}

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

use alloc::string::String;
use alloc::vec::Vec;

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
    /// Latched writers whose scope left the tree with a dormant playlist
    /// entry, keyed by the scope's stable persist path
    /// ([`ScopeRef::persist_path`]) instead of its runtime owner id.
    ///
    /// A scope owned by a node inside an entry (a pattern module's own
    /// scope, a nested playlist's sinks) is keyed by that node's id, and a
    /// reload mints a new id. Parking by path is what lets the knob come
    /// back when the entry does (multi-pattern vision D12). The persisted
    /// file already keys by path, so parked writers persist unchanged.
    parked: VecMap<(String, ChannelName), LpValue>,
    /// Monotonic count of mutations (set / clear / despawn).
    ///
    /// Persistence throttles on this rather than on the writer set
    /// itself: a clear followed by a re-write inside one window leaves an
    /// identical-looking map, and comparing `(len, newest revision)` gets
    /// that wrong. A counter cannot.
    mutations: u64,
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
        self.mutations = self.mutations.saturating_add(1);
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
        if count > 0 {
            self.mutations = self.mutations.saturating_add(1);
        }
        count
    }

    /// Clear EVERYTHING — including sink-scope writers (settled P-Q4: a
    /// playlist entry's latched value clears too) and writers parked with a
    /// dormant entry. Returns the count.
    pub fn clear_all(&mut self) -> usize {
        let count = self.writers.len() + self.parked.len();
        self.writers = VecMap::new();
        self.parked = VecMap::new();
        if count > 0 {
            self.mutations = self.mutations.saturating_add(1);
        }
        count
    }

    /// Clear one writer. Returns true when something was engaged.
    ///
    /// `VecMap::remove` needs an owned key, and cloning `channel` just to
    /// look it up is a per-call `String` copy for a store that only ever
    /// holds a handful of engaged writers. Find the matching key by
    /// reference first — O(n) over engaged writers — and clone only that one
    /// key, only on a hit.
    pub fn clear(&mut self, scope: ScopeRef, channel: &ChannelName) -> bool {
        let Some(key) = self
            .writers
            .iter()
            .map(|(key, _)| key)
            .find(|(s, c)| *s == scope && c == channel)
            .cloned()
        else {
            return false;
        };
        let cleared = self.writers.remove(&key).is_some();
        if cleared {
            self.mutations = self.mutations.saturating_add(1);
        }
        cleared
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
        if cleared > 0 {
            self.mutations = self.mutations.saturating_add(1);
        }
        cleared
    }

    /// The engaged writer for `(scope, channel)`, if any.
    ///
    /// `VecMap::get` needs `K: Borrow<Q>`, which a `(ScopeRef, ChannelName)`
    /// key cannot give a `(ScopeRef, &ChannelName)` query, so this looked up
    /// by cloning `channel` on every call — a per-frame `String` clone this
    /// store's few engaged writers do not justify. Compare by reference
    /// instead. O(n) with n = engaged writers.
    pub fn get(&self, scope: ScopeRef, channel: &ChannelName) -> Option<&PanelWriter> {
        self.writers
            .iter()
            .find(|((s, c), _)| *s == scope && c == channel)
            .map(|(_, writer)| writer)
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

    /// Mutations since construction — the persistence dirty signal.
    pub fn mutations(&self) -> u64 {
        self.mutations
    }

    /// Park every latched writer in `scope` under `persist_path`: the scope
    /// is leaving the tree with a dormant entry. Momentary writers are
    /// dropped (they would despawn anyway, and never persist). Returns the
    /// number parked.
    ///
    /// Not a mutation: the persisted set is the same before and after.
    pub fn park_scope(&mut self, scope: ScopeRef, persist_path: &str) -> usize {
        let keys: Vec<(ScopeRef, ChannelName)> = self
            .writers
            .iter()
            .filter(|((s, _), _)| *s == scope)
            .map(|(key, _)| key.clone())
            .collect();
        let mut parked = 0;
        for key in keys {
            let Some(writer) = self.writers.remove(&key) else {
                continue;
            };
            if writer.expires_at_ms.is_none() {
                self.parked
                    .insert((String::from(persist_path), key.1), writer.value);
                parked += 1;
            }
        }
        parked
    }

    /// Park one latched value under `persist_path` (restore of a dormant
    /// entry's writer from the persisted file).
    pub fn park(&mut self, persist_path: String, channel: ChannelName, value: LpValue) {
        self.parked.insert((persist_path, channel), value);
    }

    /// Whether any writer is parked under `persist_path`.
    pub fn has_parked(&self, persist_path: &str) -> bool {
        self.parked
            .iter()
            .any(|((path, _), _)| path == persist_path)
    }

    /// Engage every writer parked under `persist_path` in `scope`: the scope
    /// came back with its entry. A writer already engaged there wins over
    /// the parked value. Returns the number engaged.
    ///
    /// Not a mutation, for the same reason as [`Self::park_scope`].
    pub fn unpark_into(&mut self, persist_path: &str, scope: ScopeRef, at: Revision) -> usize {
        let keys: Vec<(String, ChannelName)> = self
            .parked
            .iter()
            .filter(|((path, _), _)| path == persist_path)
            .map(|(key, _)| key.clone())
            .collect();
        let mut engaged = 0;
        for key in keys {
            let Some(value) = self.parked.remove(&key) else {
                continue;
            };
            if self.get(scope, &key.1).is_some() {
                continue;
            }
            self.writers.insert(
                (scope, key.1),
                PanelWriter {
                    value,
                    written_at: at,
                    expires_at_ms: None,
                },
            );
            engaged += 1;
        }
        engaged
    }

    /// Every parked writer: `(persist path, channel) → value`.
    pub fn parked(&self) -> impl Iterator<Item = (&(String, ChannelName), &LpValue)> {
        self.parked.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::NodeId;

    #[test]
    fn a_parked_writer_comes_back_under_a_new_owner_id() {
        let mut store = PanelWriterStore::new();
        let old = ScopeRef::Module {
            owner: NodeId::new(7),
        };
        let speed = ChannelName(String::from("speed"));
        store.set(
            old,
            speed.clone(),
            LpValue::F32(0.25),
            Revision::new(1),
            None,
        );
        store.set(
            old,
            ChannelName(String::from("nudge")),
            LpValue::F32(1.0),
            Revision::new(1),
            Some(500),
        );
        let mutations = store.mutations();

        assert_eq!(store.park_scope(old, "/p.show/pl.playlist/a.module"), 1);
        assert!(
            store.is_empty(),
            "the momentary writer is dropped, not parked"
        );
        assert!(store.has_parked("/p.show/pl.playlist/a.module"));

        let new = ScopeRef::Module {
            owner: NodeId::new(12),
        };
        assert_eq!(
            store.unpark_into("/p.show/pl.playlist/a.module", new, Revision::new(9)),
            1
        );
        assert_eq!(
            store.get(new, &speed).map(|writer| &writer.value),
            Some(&LpValue::F32(0.25))
        );
        assert_eq!(store.parked().count(), 0);
        assert_eq!(
            store.mutations(),
            mutations,
            "parking moves a value without changing the persisted set"
        );
    }

    #[test]
    fn clear_all_clears_parked_writers_too() {
        let mut store = PanelWriterStore::new();
        store.park(
            String::from("/p.show/pl.playlist/a.module"),
            ChannelName(String::from("speed")),
            LpValue::F32(0.5),
        );
        assert_eq!(store.clear_all(), 1);
        assert_eq!(store.parked().count(), 0);
    }
}

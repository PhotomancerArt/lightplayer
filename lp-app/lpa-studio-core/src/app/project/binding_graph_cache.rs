//! The lens's cached binding graph: structure on change, values every read.
//!
//! The binding-graph probe sends the graph's structure (bindings, channel
//! identities) only when its structure revision moves, and every channel's
//! value on every read as a positional list keyed to that revision
//! (`lpc_wire::WireBindingGraphRead`). This cache is the client half. It holds
//! ONE consistent pair — a structure and the values resolved against it — and
//! never mixes halves from two revisions. The rules:
//!
//! - **`Changed` replaces** the structure, together with the values that
//!   arrived for it.
//! - **`Unchanged` keeps** the structure; the fresh values replace the old.
//! - **Values are applied only to the structure they name.** A value list
//!   whose `structure_revision` is not the held structure's (or whose length
//!   is not its channel count) is dropped, never laid onto the wrong
//!   channels.
//! - **Anything that does not add up asks again.** A mismatched value list,
//!   an `Unchanged` naming a revision the cache does not hold, an `Omitted`
//!   or a missing value list keep the last consistent pair on screen and make
//!   the next read ask the structure `Always`.
//!
//! Dropping the cache is explicit ([`BindingGraphCache::clear`]): `ProjectSync`
//! does it when its claims about the device stop holding (a reset view, an
//! unsubscribe, a new initial sync).
//!
//! Derivations that depend on the structure alone (the export lint) key off
//! [`lpc_wire::WireBindingGraph::revision`], which is now the structure
//! revision: it stands still across a steady lens, so they re-derive only on
//! a structure change.

use lpc_model::Revision;
use lpc_wire::{
    RevisionGateRead, RevisionGateResult, WireBindingGraph, WireBindingGraphRead,
    WireBusChannelValue,
};

/// The binding graph the lens holds between reads.
#[derive(Debug, Default)]
pub(crate) struct BindingGraphCache {
    /// The last consistent pair: a structure, and one value per channel
    /// resolved against it.
    held: Option<HeldBindingGraph>,
    /// The last answer did not add up: ask the structure `Always` next read.
    refetch: bool,
}

#[derive(Debug)]
struct HeldBindingGraph {
    graph: WireBindingGraph,
    values: Vec<WireBusChannelValue>,
}

impl BindingGraphCache {
    /// What the next probe should ask of the structure: `IfChanged` against
    /// the held revision, `Always` when nothing is held or the last answer
    /// did not add up.
    pub(crate) fn structure_read(&self) -> RevisionGateRead {
        match &self.held {
            Some(held) if !self.refetch => RevisionGateRead::IfChanged {
                known_revision: Some(held.graph.revision),
            },
            _ => RevisionGateRead::Always,
        }
    }

    /// Fold one probe answer in (see the module docs for the rules).
    pub(crate) fn apply(&mut self, read: &WireBindingGraphRead) {
        let structure = match &read.structure {
            RevisionGateResult::Changed(graph) => Some(graph),
            RevisionGateResult::Unchanged { revision } => self
                .held
                .as_ref()
                .map(|held| &held.graph)
                .filter(|graph| graph.revision == *revision),
            RevisionGateResult::Omitted => None,
        };
        let values = structure.and_then(|graph| {
            read.values
                .as_ref()
                .filter(|values| fits(graph, values.structure_revision, &values.values))
                .map(|values| values.values.clone())
        });
        let Some(values) = values else {
            self.refetch = true;
            return;
        };
        match &read.structure {
            RevisionGateResult::Changed(graph) => {
                self.held = Some(HeldBindingGraph {
                    graph: graph.clone(),
                    values,
                });
            }
            // `values` only exists when the held structure is the one named.
            RevisionGateResult::Unchanged { .. } | RevisionGateResult::Omitted => {
                if let Some(held) = &mut self.held {
                    held.values = values;
                }
            }
        }
        self.refetch = false;
    }

    /// Forget everything held; the next read asks `Always`.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The held structure.
    pub(crate) fn graph(&self) -> Option<&WireBindingGraph> {
        self.held.as_ref().map(|held| &held.graph)
    }

    /// One value per [`WireBindingGraph::channels`] row of the held
    /// structure, in that order; empty when nothing is held.
    pub(crate) fn values(&self) -> &[WireBusChannelValue] {
        self.held.as_ref().map_or(&[], |held| &held.values)
    }

    /// Hold `graph` and `values` directly (test fixture path). `values` is
    /// padded with `Unresolved` to the channel count, so a fixture that does
    /// not care about values can pass none.
    #[cfg(test)]
    pub(crate) fn set_for_test(
        &mut self,
        graph: WireBindingGraph,
        mut values: Vec<WireBusChannelValue>,
    ) {
        values.resize(graph.channels.len(), WireBusChannelValue::Unresolved);
        self.held = Some(HeldBindingGraph { graph, values });
        self.refetch = false;
    }
}

/// Whether a value list belongs to `graph`: resolved against its revision,
/// one value per channel.
fn fits(graph: &WireBindingGraph, revision: Revision, values: &[WireBusChannelValue]) -> bool {
    revision == graph.revision && values.len() == graph.channels.len()
}

#[cfg(test)]
mod tests {
    use lpc_model::LpValue;
    use lpc_wire::{WireBusChannel, WireBusChannelValues};

    use super::*;

    #[test]
    fn a_changed_structure_is_held_with_its_values_and_gates_the_next_read() {
        let mut cache = BindingGraphCache::default();
        assert_eq!(cache.structure_read(), RevisionGateRead::Always);

        cache.apply(&changed(graph(4, &["time"]), &[0.5]));
        assert_eq!(cache.graph(), Some(&graph(4, &["time"])));
        assert_eq!(cache.values(), &[value(0.5)]);
        assert_eq!(
            cache.structure_read(),
            RevisionGateRead::IfChanged {
                known_revision: Some(Revision::new(4))
            }
        );
    }

    #[test]
    fn unchanged_keeps_the_structure_and_takes_the_new_values() {
        let mut cache = BindingGraphCache::default();
        cache.apply(&changed(graph(4, &["time"]), &[0.5]));

        cache.apply(&unchanged(4, 4, &[0.75]));
        assert_eq!(cache.graph(), Some(&graph(4, &["time"])));
        assert_eq!(cache.values(), &[value(0.75)]);
        assert_eq!(
            cache.structure_read(),
            RevisionGateRead::IfChanged {
                known_revision: Some(Revision::new(4))
            }
        );
    }

    /// Values keyed to a structure the cache does not hold are never laid
    /// onto the held channels: the last consistent pair stays, and the next
    /// read asks `Always`.
    #[test]
    fn a_value_structure_mismatch_drops_the_values_and_asks_always() {
        let mut cache = BindingGraphCache::default();
        cache.apply(&changed(graph(4, &["time"]), &[0.5]));

        cache.apply(&unchanged(4, 9, &[0.75]));
        assert_eq!(cache.values(), &[value(0.5)], "the stale list was dropped");
        assert_eq!(cache.graph(), Some(&graph(4, &["time"])));
        assert_eq!(cache.structure_read(), RevisionGateRead::Always);

        // The `Always` answer puts it right.
        cache.apply(&changed(graph(9, &["time", "speed"]), &[0.75, 2.0]));
        assert_eq!(cache.values(), &[value(0.75), value(2.0)]);
        assert_eq!(
            cache.structure_read(),
            RevisionGateRead::IfChanged {
                known_revision: Some(Revision::new(9))
            }
        );
    }

    #[test]
    fn an_unchanged_naming_another_revision_asks_always() {
        let mut cache = BindingGraphCache::default();
        cache.apply(&changed(graph(4, &["time"]), &[0.5]));

        cache.apply(&unchanged(5, 5, &[0.75]));
        assert_eq!(cache.values(), &[value(0.5)]);
        assert_eq!(cache.structure_read(), RevisionGateRead::Always);
    }

    #[test]
    fn a_value_list_of_the_wrong_length_is_never_applied() {
        let mut cache = BindingGraphCache::default();
        cache.apply(&changed(graph(4, &["time", "speed"]), &[0.5]));
        assert_eq!(cache.graph(), None, "nothing consistent was ever held");
        assert_eq!(cache.structure_read(), RevisionGateRead::Always);
    }

    #[test]
    fn clear_forgets_everything() {
        let mut cache = BindingGraphCache::default();
        cache.apply(&changed(graph(4, &["time"]), &[0.5]));
        cache.clear();
        assert_eq!(cache.graph(), None);
        assert!(cache.values().is_empty());
        assert_eq!(cache.structure_read(), RevisionGateRead::Always);
    }

    fn graph(revision: i64, channels: &[&str]) -> WireBindingGraph {
        WireBindingGraph {
            revision: Revision::new(revision),
            bindings: Vec::new(),
            channels: channels
                .iter()
                .map(|name| WireBusChannel {
                    scope: None,
                    name: (*name).to_string(),
                    kind: None,
                    providers: Vec::new(),
                    consumers: Vec::new(),
                    primary_visual: false,
                })
                .collect(),
        }
    }

    fn value(value: f32) -> WireBusChannelValue {
        WireBusChannelValue::Value(LpValue::F32(value))
    }

    fn values(structure_revision: i64, values: &[f32]) -> Option<WireBusChannelValues> {
        Some(WireBusChannelValues {
            structure_revision: Revision::new(structure_revision),
            values: values.iter().copied().map(value).collect(),
        })
    }

    fn changed(graph: WireBindingGraph, channel_values: &[f32]) -> WireBindingGraphRead {
        let revision = graph.revision.as_i64();
        WireBindingGraphRead {
            structure: RevisionGateResult::Changed(graph),
            values: values(revision, channel_values),
        }
    }

    fn unchanged(
        revision: i64,
        values_revision: i64,
        channel_values: &[f32],
    ) -> WireBindingGraphRead {
        WireBindingGraphRead {
            structure: RevisionGateResult::Unchanged {
                revision: Revision::new(revision),
            },
            values: values(values_revision, channel_values),
        }
    }
}

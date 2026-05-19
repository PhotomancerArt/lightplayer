//! Derived indexes over bindings stored on node entries.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lpc_model::{ChannelName, Kind, NodeId, SlotPath};

use crate::dataflow::binding::{
    BindingEntry, BindingError, BindingRef, BindingTarget, channels_touched,
};

use super::NodeEntry;

#[derive(Clone, Debug, Default)]
pub(super) struct NodeBindingIndex {
    consumed_targets: BTreeMap<(NodeId, SlotPath), Vec<BindingRef>>,
    bus_targets: BTreeMap<ChannelName, Vec<BindingRef>>,
}

impl NodeBindingIndex {
    pub(super) fn rebuild<N>(entries: &[Option<NodeEntry<N>>]) -> Result<Self, BindingError> {
        let mut index = Self::default();
        let mut channel_kinds: BTreeMap<ChannelName, Kind> = BTreeMap::new();

        for entry in entries.iter().filter_map(|entry| entry.as_ref()) {
            for (binding_index, binding) in entry.bindings.value().iter().enumerate() {
                let binding_ref = BindingRef::new(entry.id, binding_index);
                for channel in channels_touched(&binding.source, &binding.target) {
                    if let Some(established) = channel_kinds.get(&channel) {
                        if *established != binding.kind {
                            return Err(BindingError::KindMismatch {
                                channel,
                                established: *established,
                                attempted: binding.kind,
                            });
                        }
                    } else {
                        channel_kinds.insert(channel, binding.kind);
                    }
                }

                match &binding.target {
                    BindingTarget::ConsumedSlot { node, slot } => {
                        index
                            .consumed_targets
                            .entry((*node, slot.clone()))
                            .or_default()
                            .push(binding_ref);
                    }
                    BindingTarget::BusChannel(channel) => {
                        index
                            .bus_targets
                            .entry(channel.clone())
                            .or_default()
                            .push(binding_ref);
                    }
                }
            }
        }

        Ok(index)
    }

    pub(super) fn consumed_targets(&self, node: NodeId, slot: &SlotPath) -> &[BindingRef] {
        self.consumed_targets
            .get(&(node, slot.clone()))
            .map_or(&[], Vec::as_slice)
    }

    pub(super) fn bus_targets(&self, channel: &ChannelName) -> &[BindingRef] {
        self.bus_targets.get(channel).map_or(&[], Vec::as_slice)
    }
}

pub(super) fn binding_by_ref<N>(
    entries: &[Option<NodeEntry<N>>],
    binding_ref: BindingRef,
) -> Option<&BindingEntry> {
    entries
        .get(binding_ref.owner.0 as usize)
        .and_then(|entry| entry.as_ref())
        .and_then(|entry| entry.bindings.value().get(binding_ref.index))
}

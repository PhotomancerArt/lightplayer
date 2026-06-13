//! Derived indexes over bindings stored on node entries.

use alloc::vec::Vec;
use lp_collection::VecMap;

use lpc_model::{ChannelName, Kind, NodeId, SlotPath};

use crate::dataflow::binding::{
    BindingEntry, BindingError, BindingRef, BindingTarget, channels_touched,
};

use super::RuntimeNodeEntry;

#[derive(Clone, Debug, Default)]
pub(super) struct NodeBindingIndex {
    consumed_targets: VecMap<(NodeId, SlotPath), Vec<BindingRef>>,
    bus_targets: VecMap<ChannelName, Vec<BindingRef>>,
    channel_kinds: VecMap<ChannelName, Kind>,
}

impl NodeBindingIndex {
    pub(super) fn rebuild<N>(
        entries: &[Option<RuntimeNodeEntry<N>>],
    ) -> Result<Self, BindingError> {
        let mut index = Self::default();

        for entry in entries.iter().filter_map(|entry| entry.as_ref()) {
            for (binding_index, binding) in entry.bindings.value().iter().enumerate() {
                let binding_ref = BindingRef::new(entry.id, binding_index);
                index.insert_binding(binding_ref, binding)?;
            }
        }

        Ok(index)
    }

    pub(super) fn insert_binding(
        &mut self,
        binding_ref: BindingRef,
        binding: &BindingEntry,
    ) -> Result<(), BindingError> {
        for channel in channels_touched(&binding.source, &binding.target) {
            if let Some(established) = self.channel_kinds.get(channel) {
                if *established != binding.kind {
                    return Err(BindingError::KindMismatch {
                        channel: channel.clone(),
                        established: *established,
                        attempted: binding.kind,
                    });
                }
            }
        }

        for channel in channels_touched(&binding.source, &binding.target) {
            self.channel_kinds
                .entry(channel.clone())
                .or_insert(binding.kind);
        }

        match &binding.target {
            BindingTarget::ConsumedSlot { node, slot } => {
                self.consumed_targets
                    .entry((*node, slot.clone()))
                    .or_default()
                    .push(binding_ref);
            }
            BindingTarget::BusChannel(channel) => {
                self.bus_targets
                    .entry(channel.clone())
                    .or_default()
                    .push(binding_ref);
            }
        }

        Ok(())
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
    entries: &[Option<RuntimeNodeEntry<N>>],
    binding_ref: BindingRef,
) -> Option<&BindingEntry> {
    entries
        .get(binding_ref.owner.0 as usize)
        .and_then(|entry| entry.as_ref())
        .and_then(|entry| entry.bindings.value().get(binding_ref.index))
}

//! Public runtime state shape for playlist nodes.

use crate::{Slotted, ValueSlot, VisualProduct, VisualProductSlot};

/// Runtime state exposed by a playlist node.
#[derive(Default, Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct PlaylistState {
    /// Renderable visual output produced by this playlist node.
    #[slot(produced, default_bind = "bus:visual.out")]
    pub output: VisualProductSlot,

    /// Seconds since the current entry became active.
    #[slot(produced)]
    pub entry_time: ValueSlot<f32>,

    /// Normalized progress through the current timed entry, or -1.0 for untimed entries.
    #[slot(produced)]
    pub entry_progress: ValueSlot<f32>,

    /// Current entry key.
    #[slot(produced)]
    pub active_entry: ValueSlot<u32>,
}

impl PlaylistState {
    pub fn new(
        output: VisualProduct,
        entry_time: f32,
        entry_progress: f32,
        active_entry: u32,
    ) -> Self {
        Self {
            output: VisualProductSlot::new(output),
            entry_time: ValueSlot::new(entry_time),
            entry_progress: ValueSlot::new(entry_progress),
            active_entry: ValueSlot::new(active_entry),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, SlotDirection, SlotShape, StaticSlotShape};

    #[test]
    fn playlist_state_fields_are_produced() {
        let SlotShape::Record { fields, .. } = PlaylistState::slot_shape() else {
            panic!("record shape");
        };

        for name in ["output", "entry_time", "entry_progress", "active_entry"] {
            let field = fields
                .iter()
                .find(|field| field.name.as_str() == name)
                .expect("playlist state field");
            assert_eq!(field.semantics.direction, SlotDirection::Produced);
        }
    }

    #[test]
    fn playlist_state_new_sets_public_outputs() {
        let state = PlaylistState::new(VisualProduct::new(NodeId::new(4), 0), 1.5, 0.375, 2);

        assert_eq!(state.output.value().node(), NodeId::new(4));
        assert_eq!(*state.entry_time.value(), 1.5);
        assert_eq!(*state.entry_progress.value(), 0.375);
        assert_eq!(*state.active_entry.value(), 2);
    }
}

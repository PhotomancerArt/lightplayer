use super::PlaylistEntry;
use crate::{
    BindingDefs, ControlMessage, MapSlot, PositiveF32, PositiveF32Slot, Slotted, ValueSlot,
};

/// Authored playlist visual selector node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct PlaylistDef {
    /// Authored slot bindings for playlist-level inputs and visual output.
    pub bindings: BindingDefs,

    /// Global graph time in seconds.
    #[slot(consumed)]
    pub time: ValueSlot<f32>,

    /// Trigger messages that start or restart entries (routed by entry
    /// `trigger_ids`).
    #[slot(
        consumed,
        merge = "by_key",
        map(key = "u32", value_ref = "lp::control::Message")
    )]
    pub trigger: MapSlot<u32, ControlMessage>,

    /// Entry shown when no triggered sequence is active.
    pub idle_entry: ValueSlot<u32>,

    /// Default outgoing crossfade duration in seconds.
    pub default_fade: PositiveF32Slot,

    /// Authored entries keyed by stable playlist position.
    pub entries: MapSlot<u32, PlaylistEntry>,
}

impl Default for PlaylistDef {
    fn default() -> Self {
        Self {
            bindings: BindingDefs::default(),
            time: default_time(),
            trigger: MapSlot::default(),
            idle_entry: default_idle_entry(),
            default_fade: default_fade(),
            entries: MapSlot::default(),
        }
    }
}

impl PlaylistDef {
    pub const KIND: &'static str = "playlist";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Playlist
    }
}

fn default_time() -> ValueSlot<f32> {
    ValueSlot::new(0.0)
}

fn default_idle_entry() -> ValueSlot<u32> {
    ValueSlot::new(1)
}

fn default_fade() -> PositiveF32Slot {
    PositiveF32Slot::new(PositiveF32(0.25))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeDef, NodeKind, SlotDirection, SlotMerge, SlotShape, StaticSlotShape};

    #[test]
    fn playlist_def_parses_minimal_defaults() {
        let def = NodeDef::from_json_str(r#"{ "kind": "Playlist" }"#).expect("playlist");

        let NodeDef::Playlist(def) = def else {
            panic!("playlist def");
        };
        assert_eq!(*def.time.value(), 0.0);
        assert_eq!(*def.idle_entry.value(), 1);
        assert_eq!(def.default_fade.value().0, 0.25);
        assert!(def.entries.is_empty());
    }

    #[test]
    fn playlist_time_shape_is_consumed_latest() {
        let SlotShape::Record { fields, .. } = PlaylistDef::slot_shape() else {
            panic!("record shape");
        };
        let time = fields
            .iter()
            .find(|field| field.name.as_str() == "time")
            .expect("time field");

        assert_eq!(time.semantics.direction, SlotDirection::Consumed);
        assert_eq!(time.semantics.merge, SlotMerge::Latest);
    }

    #[test]
    fn playlist_trigger_shape_is_consumed_by_key() {
        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(crate::ControlMessage::SHAPE_ID),
            Some(crate::CONTROL_MESSAGE_SHAPE_NAME)
        );

        let SlotShape::Record { fields, .. } = PlaylistDef::slot_shape() else {
            panic!("record shape");
        };
        let trigger = fields
            .iter()
            .find(|field| field.name.as_str() == "trigger")
            .expect("trigger field");

        assert_eq!(trigger.semantics.direction, SlotDirection::Consumed);
        assert_eq!(trigger.semantics.merge, SlotMerge::ByKey);
    }

    #[test]
    fn node_def_delegates_playlist_kind() {
        let def = NodeDef::Playlist(PlaylistDef::default());

        assert_eq!(def.kind(), NodeKind::Playlist);
        assert_eq!(def.kind_name(), "playlist");
        assert_eq!(def.variant_name(), "Playlist");
        assert!(def.as_playlist().is_some());
    }
}

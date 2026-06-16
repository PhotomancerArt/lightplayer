use alloc::string::String;

use crate::{
    BindingDefs, ControlMessage, MapSlot, NodeInvocation, NodeInvocationSlot, OptionSlot,
    PositiveF32Slot, Slotted, ValueSlot,
};

/// One authored playlist entry.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct PlaylistEntry {
    /// Entry-local bindings, registered against the owning playlist entry slot.
    pub bindings: BindingDefs,

    /// Trigger messages that start or restart this entry.
    #[slot(
        consumed,
        merge = "by_key",
        map(key = "u32", value_ref = "lp::control::Message")
    )]
    pub trigger: MapSlot<u32, ControlMessage>,

    /// Optional child node name.
    pub name: OptionSlot<ValueSlot<String>>,

    /// Duration in seconds before the playlist advances.
    pub duration: OptionSlot<PositiveF32Slot>,

    /// Outgoing crossfade duration override in seconds.
    pub fade_after: OptionSlot<PositiveF32Slot>,

    /// Visual child node position owned by this playlist entry.
    pub node: NodeInvocationSlot,
}

impl Default for PlaylistEntry {
    fn default() -> Self {
        Self {
            bindings: BindingDefs::default(),
            trigger: MapSlot::default(),
            name: OptionSlot::none(),
            duration: OptionSlot::none(),
            fade_after: OptionSlot::none(),
            node: NodeInvocationSlot::new(NodeInvocation::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BindingRef, NodeDef, SlotDirection, SlotMerge, SlotShape, StaticSlotShape};

    #[test]
    fn playlist_entry_parses_path_child_and_trigger_binding() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.2]
name = "active"
duration = 4.0
fade_after = 0.8
[entries.2.node]
ref = "./active.toml"

[entries.2.bindings.trigger]
source = "bus#trigger"
"#,
        )
        .expect("playlist");

        let NodeDef::Playlist(def) = def else {
            panic!("playlist def");
        };
        let entry = def.entries.entries.get(&2).expect("entry");
        assert_eq!(entry.name.data.as_ref().unwrap().value().as_str(), "active");
        assert_eq!(entry.duration.data.as_ref().unwrap().value().0, 4.0);
        assert!(matches!(entry.node.value(), NodeInvocation::Ref(_)));
        assert!(matches!(
            entry.bindings.entries()["trigger"].source_ref(),
            Some(BindingRef::Bus(_))
        ));
    }

    #[test]
    fn playlist_entry_parses_inline_child() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.2]
name = "active"
duration = 4.0

[entries.2.node.def]
kind = "Shader"
source = { path = "active.glsl" }
"#,
        )
        .expect("playlist");

        let NodeDef::Playlist(def) = def else {
            panic!("playlist def");
        };
        let entry = def.entries.entries.get(&2).expect("entry");
        assert!(matches!(
            entry.node.value().inline_def(),
            Some(NodeDef::Shader(_))
        ));
    }

    #[test]
    fn playlist_entry_trigger_shape_is_consumed_by_key() {
        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(ControlMessage::SHAPE_ID),
            Some(crate::CONTROL_MESSAGE_SHAPE_NAME)
        );

        let SlotShape::Record { fields, .. } = PlaylistEntry::slot_shape() else {
            panic!("record shape");
        };
        let trigger = fields
            .iter()
            .find(|field| field.name.as_str() == "trigger")
            .expect("trigger field");

        assert_eq!(trigger.semantics.direction, SlotDirection::Consumed);
        assert_eq!(trigger.semantics.merge, SlotMerge::ByKey);
    }
}

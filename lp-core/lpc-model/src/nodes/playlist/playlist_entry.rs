use alloc::string::String;

use crate::{
    NodeInvocation, NodeInvocationSlot, OptionSlot, PositiveF32Slot, Slotted, U32ListSlot,
    ValueSlot,
};

/// One authored playlist entry.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct PlaylistEntry {
    /// Optional child node name.
    pub name: OptionSlot<ValueSlot<String>>,

    /// Trigger message ids (button ids) that start or restart this entry.
    ///
    /// Absent means the entry is never triggered. When several entries claim
    /// the same id, the lowest entry index wins.
    pub trigger_ids: OptionSlot<U32ListSlot>,

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
            name: OptionSlot::none(),
            trigger_ids: OptionSlot::none(),
            duration: OptionSlot::none(),
            fade_after: OptionSlot::none(),
            node: NodeInvocationSlot::new(NodeInvocation::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeDef;

    #[test]
    fn playlist_entry_parses_path_child_and_trigger_ids() {
        let def = NodeDef::from_json_str(
            r#"{
  "kind": "Playlist",
  "entries": {
    "2": {
      "name": "active",
      "trigger_ids": [1],
      "duration": 4.0,
      "fade_after": 0.8,
      "node": { "ref": "./active.json" }
    }
  }
}"#,
        )
        .expect("playlist");

        let NodeDef::Playlist(def) = def else {
            panic!("playlist def");
        };
        let entry = def.entries.entries.get(&2).expect("entry");
        assert_eq!(entry.name.data.as_ref().unwrap().value().as_str(), "active");
        assert_eq!(entry.duration.data.as_ref().unwrap().value().0, 4.0);
        assert!(matches!(entry.node.value(), NodeInvocation::Ref(_)));
        assert_eq!(
            entry
                .trigger_ids
                .data
                .as_ref()
                .unwrap()
                .value()
                .0
                .as_slice(),
            &[1]
        );
    }

    #[test]
    fn playlist_entry_without_trigger_ids_is_untriggered() {
        let def = NodeDef::from_json_str(
            r#"{
  "kind": "Playlist",
  "entries": {
    "1": {
      "name": "idle",
      "node": { "ref": "./idle.json" }
    }
  }
}"#,
        )
        .expect("playlist");

        let NodeDef::Playlist(def) = def else {
            panic!("playlist def");
        };
        let entry = def.entries.entries.get(&1).expect("entry");
        assert!(entry.trigger_ids.data.is_none());
    }

    #[test]
    fn playlist_entry_rejects_inline_child() {
        let err = NodeDef::from_json_str(
            r#"{
  "kind": "Playlist",
  "entries": {
    "2": {
      "name": "active",
      "duration": 4.0,
      "node": {
        "def": { "kind": "Shader", "source": "active.glsl" }
      }
    }
  }
}"#,
        )
        .expect_err("inline child definitions are not supported");
        assert!(alloc::format!("{err}").contains("def"), "{err}");
    }
}

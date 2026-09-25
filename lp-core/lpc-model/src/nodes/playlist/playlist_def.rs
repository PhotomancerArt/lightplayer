use super::{PlaylistEntry, PlaylistTour};
use crate::{
    BindingDefs, ControlMessage, MapSlot, OptionSlot, PositiveF32, PositiveF32Slot, Slotted,
    TimeProductSlot, U32ListSlot, ValueSlot,
};

/// Authored playlist visual selector node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct PlaylistDef {
    /// Authored slot bindings for playlist-level inputs and visual output.
    pub bindings: BindingDefs,

    /// Graph timebase the playlist schedules against — the scope's time
    /// product, queried for effective seconds. `entry_time`/`entry_progress`
    /// stay plain f32: they are entry-relative, not the project clock.
    #[slot(consumed)]
    pub time: TimeProductSlot,

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

    /// How the playlist walks its entries over time: hold (absent, the
    /// playlist as it always was) or cycle `{ step_seconds, fade_seconds }`
    /// (vision D13). The authored value is the default; Play mode overrides
    /// it with a remembered panel write on `bus:playlist.tour`, which is why
    /// the default binding is promoted to the panel (plan A1).
    #[slot(consumed, default_bind = "bus:playlist.tour", panel = "show")]
    pub tour: OptionSlot<ValueSlot<PlaylistTour>>,

    /// Entry keys switched off in the tour, and ignored by triggers and
    /// next/prev. The authored list is the default; a panel write on
    /// `bus:playlist.skip` replaces it (plan A1).
    ///
    /// One whole list, not a channel per entry: the bus has no
    /// read-modify-write primitive, so a record written by several producers
    /// races (the clock transport ADR). One value written whole by one
    /// writer — Studio's pattern picker — does not.
    #[slot(consumed, default_bind = "bus:playlist.skip", panel = "show")]
    pub skip: OptionSlot<U32ListSlot>,

    /// Trigger message ids (button ids) that step to the next enabled entry,
    /// wrapping. Matched in the same pass as the entries' `trigger_ids`.
    pub next_trigger_ids: OptionSlot<U32ListSlot>,

    /// Trigger message ids that step to the previous enabled entry, wrapping.
    pub prev_trigger_ids: OptionSlot<U32ListSlot>,

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
            tour: OptionSlot::none(),
            skip: OptionSlot::none(),
            next_trigger_ids: OptionSlot::none(),
            prev_trigger_ids: OptionSlot::none(),
            entries: MapSlot::default(),
        }
    }
}

impl PlaylistDef {
    pub const KIND: &'static str = "playlist";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Playlist
    }

    /// The entry this playlist plays when nothing else chose one: its
    /// authored `idle_entry` when that names an authored entry, otherwise the
    /// first authored entry by key — a dangling `idle_entry` plays the first
    /// entry rather than nothing (multi-pattern plan, director ruling DD7).
    /// With no entries at all, the authored value.
    ///
    /// This is the entry the registry makes resident at load and the entry
    /// the runtime playlist treats as idle, so the two always agree.
    pub fn effective_idle_entry(&self) -> u32 {
        let idle = *self.idle_entry.value();
        if self.entries.entries.contains_key(&idle) {
            return idle;
        }
        self.entries.entries.keys().copied().min().unwrap_or(idle)
    }
}

fn default_time() -> TimeProductSlot {
    TimeProductSlot::default()
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
        assert_eq!(*def.time.value(), crate::TimeProduct::default());
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
    fn playlist_def_parses_tour_skip_and_step_triggers() {
        let def = NodeDef::from_json_str(
            r#"{
  "kind": "Playlist",
  "tour": { "kind": "cycle", "step_seconds": 20, "fade_seconds": 1.5 },
  "skip": [3, 5],
  "next_trigger_ids": [7],
  "prev_trigger_ids": [8, 9]
}"#,
        )
        .expect("playlist");

        let NodeDef::Playlist(def) = def else {
            panic!("playlist def");
        };
        assert_eq!(
            *def.tour.data.as_ref().expect("tour").value(),
            PlaylistTour::Cycle {
                step_seconds: 20.0,
                fade_seconds: 1.5
            }
        );
        let list = |slot: &OptionSlot<U32ListSlot>| slot.data.as_ref().unwrap().value().0.clone();
        assert_eq!(list(&def.skip), [3, 5]);
        assert_eq!(list(&def.next_trigger_ids), [7]);
        assert_eq!(list(&def.prev_trigger_ids), [8, 9]);
    }

    /// The additive fields are absent by default, so a playlist authored
    /// before them reads — and writes back — unchanged.
    #[test]
    fn playlist_def_omits_absent_tour_fields() {
        let def = PlaylistDef::default();
        assert!(def.tour.data.is_none());
        assert!(def.skip.data.is_none());
        assert!(def.next_trigger_ids.data.is_none());
        assert!(def.prev_trigger_ids.data.is_none());
    }

    /// Tour and skip are consumed, default-bound to their playlist channels
    /// and promoted to the panel: the authored value is the default, and a
    /// Play-mode panel write overrides it (plan A1).
    #[test]
    fn tour_and_skip_are_panel_public_default_bindings() {
        let SlotShape::Record { fields, .. } = PlaylistDef::slot_shape() else {
            panic!("record shape");
        };
        for (name, channel) in [("tour", "bus:playlist.tour"), ("skip", "bus:playlist.skip")] {
            let field = fields
                .iter()
                .find(|field| field.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} field"));
            assert_eq!(field.semantics.direction, SlotDirection::Consumed, "{name}");
            assert_eq!(
                field.default_bind.as_deref(),
                Some(channel),
                "{name}"
            );
            assert!(field.panel.is_some(), "{name} is promoted to the panel");
        }
    }

    #[test]
    fn node_def_delegates_playlist_kind() {
        let def = NodeDef::Playlist(PlaylistDef::default());

        assert_eq!(def.kind(), NodeKind::Playlist);
        assert_eq!(def.kind_name(), "playlist");
        assert_eq!(def.variant_name(), "Playlist");
        assert!(def.as_playlist().is_some());
    }

    #[test]
    fn effective_idle_entry_falls_back_to_the_first_authored_key() {
        let parse = |json: &str| {
            let NodeDef::Playlist(def) = NodeDef::from_json_str(json).expect("playlist") else {
                panic!("playlist def");
            };
            def
        };
        let entries = r#""entries": {
            "3": { "node": { "ref": "./c.json" } },
            "2": { "node": { "ref": "./b.json" } }
        }"#;

        let authored = parse(&alloc::format!(
            r#"{{ "kind": "Playlist", "idle_entry": 3, {entries} }}"#
        ));
        assert_eq!(
            authored.effective_idle_entry(),
            3,
            "an authored idle entry wins"
        );

        let dangling = parse(&alloc::format!(
            r#"{{ "kind": "Playlist", "idle_entry": 9, {entries} }}"#
        ));
        assert_eq!(dangling.effective_idle_entry(), 2, "dangling → lowest key");

        let empty = parse(r#"{ "kind": "Playlist", "idle_entry": 9 }"#);
        assert_eq!(
            empty.effective_idle_entry(),
            9,
            "no entries → authored value"
        );
    }
}

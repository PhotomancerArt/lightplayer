use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    FieldSlot, FieldSlotMut, FromLpValue, LpType, LpValue, OrderedF32, Revision, SlotDataAccess,
    SlotDataAccessMut, SlotEnumOption, SlotMapValueAccessMut, SlotMerge, SlotMeta,
    SlotRecordAccess, SlotRecordAccessMut, SlotRole, SlotSemantics, SlotShape, SlotShapeId,
    SlotValue, SlotValueShape, StaticLpType, StaticSlotEnumOption, StaticSlotFieldShape,
    StaticSlotMeta, StaticSlotShape, StaticSlotShapeDescriptor, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

const FRAME_SECONDS_60HZ: f32 = 1.0 / 60.0;

/// Native shape name for [`ClockTransport`].
pub const CLOCK_TRANSPORT_SHAPE_NAME: &str = "lp::clock::Transport";

/// Native shape name for [`PlayState`].
pub const CLOCK_PLAY_STATE_SHAPE_NAME: &str = "lp::clock::PlayState";

/// Declared default binding for [`ClockTransport::play_state`].
pub const CLOCK_PLAY_STATE_DEFAULT_BIND: &str = "bus:clock.play_state";
/// Declared default binding for [`ClockTransport::rate`].
pub const CLOCK_RATE_DEFAULT_BIND: &str = "bus:clock.rate";
/// Declared default binding for [`ClockTransport::scrub_offset_seconds`].
pub const CLOCK_SCRUB_DEFAULT_BIND: &str = "bus:clock.scrub";

/// The clock transport's run/pause setpoint.
///
/// A **state noun**, deliberately, not a verb (D20): the channel carries the
/// *desired* transport state, and a consumer that reads it late still learns
/// the right thing. Commands ("toggle", "tap tempo") are `trigger`-channel
/// business, where a missed message means a missed event.
///
/// This is the REQUESTED state. The EFFECTIVE state — what the clock is
/// actually doing — reads off the produced side (`ClockState` / the
/// `TimeProduct` behind `bus:time`), which is what the tape strip's motion
/// already renders. Today the two never disagree; the split exists so a
/// future quantized pause or external sync has somewhere honest to land.
///
/// It rides slots as a string leaf with a discrete-choice editor hint, the
/// way [`crate::nodes::shader::FloatMode`] and [`crate::Waveform`] do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PlayState {
    /// The transport advances the clock.
    #[default]
    Playing,
    /// The transport holds the clock still.
    Paused,
}

impl PlayState {
    /// Snake-case wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Paused => "paused",
        }
    }

    /// Parse a snake-case wire tag.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "playing" => Some(Self::Playing),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }

    /// Whether the transport is advancing.
    #[must_use]
    pub const fn is_playing(self) -> bool {
        matches!(self, Self::Playing)
    }

    /// The other state — what a run/pause toggle lands on.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Playing => Self::Paused,
            Self::Paused => Self::Playing,
        }
    }

    /// Every state, in declaration order (pickers, tests).
    #[must_use]
    pub const fn all() -> &'static [PlayState] {
        &[PlayState::Playing, PlayState::Paused]
    }
}

impl core::fmt::Display for PlayState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ToLpValue for PlayState {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for PlayState {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(value)
                .ok_or_else(|| ValueRootError::new(alloc::format!("unknown play state {value:?}"))),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for PlayState {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(CLOCK_PLAY_STATE_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: <PlayState as SlotValue>::SHAPE_ID,
            ty: StaticLpType::String,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Dropdown {
                options: &[
                    StaticSlotEnumOption {
                        value: "playing",
                        label: "Playing",
                    },
                    StaticSlotEnumOption {
                        value: "paused",
                        label: "Paused",
                    },
                ],
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <PlayState as SlotValue>::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: vec![
                    SlotEnumOption::new("playing", "Playing"),
                    SlotEnumOption::new("paused", "Paused"),
                ],
            },
        }
    }
}

/// The project clock's transient transport: play state, rate, scrub offset.
///
/// The transport lives in authored node-def slot data so the UI can mutate it
/// through the same path as ordinary config. Its fields' slot role marks them
/// as `Debug`: writable but never persisted, since they are runtime transport
/// controls, not durable defaults.
///
/// **One record in the model, three wires on the bus** (P6/D20). Each leaf
/// declares its own `default_bind` onto a `clock.*` channel, so anything can
/// modulate one dimension without a pack/unpack adapter and without the bus
/// needing a read-modify-write primitive it does not have. The record itself
/// is what carries `panel = "show"` (on `ClockDef::transport`), which is how
/// the three leaf channels group into ONE panel control.
///
/// The record is still named ([`CLOCK_TRANSPORT_SHAPE_NAME`]) and still
/// carries an [`LpValue`] struct form: that identity is what a transport
/// *widget* keys off, and what lets the whole record travel as one value
/// where a consumer genuinely wants it whole.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockTransport {
    pub play_state: ValueSlot<PlayState>,
    pub rate: ValueSlot<f32>,
    pub scrub_offset_seconds: ValueSlot<f32>,
}

impl Default for ClockTransport {
    fn default() -> Self {
        Self {
            play_state: default_play_state(),
            rate: default_rate(),
            scrub_offset_seconds: ValueSlot::new(0.0),
        }
    }
}

/// The transport leaves are dataflow endpoints, not node-local bookkeeping:
/// each one sources FROM its `clock.*` channel at fallback priority (an
/// authored binding on the same leaf wins), and the clock node reads them
/// through the consumed-slot accessor path. `Debug` role keeps them writable
/// but transient — a transport is never persisted.
const TRANSPORT_LEAF_SEMANTICS: SlotSemantics = SlotSemantics::consumed(SlotMerge::Latest);

/// Borrowed record descriptor shared by the [`FieldSlot`] mount and the named
/// [`StaticSlotShape`] identity — one declaration, so the two cannot drift.
const STATIC_TRANSPORT_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
    Some(&StaticSlotShapeDescriptor::Record {
        meta: StaticSlotMeta::EMPTY,
        fields: &[
            StaticSlotFieldShape {
                name: "play_state",
                shape: &StaticSlotShapeDescriptor::Value {
                    shape: static_clock_play_state_shape(),
                },
                semantics: TRANSPORT_LEAF_SEMANTICS,
                role: SlotRole::Debug,
                default_bind: Some(CLOCK_PLAY_STATE_DEFAULT_BIND),
                panel: None,
            },
            StaticSlotFieldShape {
                name: "rate",
                shape: &StaticSlotShapeDescriptor::Value {
                    shape: static_clock_rate_shape(),
                },
                semantics: TRANSPORT_LEAF_SEMANTICS,
                role: SlotRole::Debug,
                default_bind: Some(CLOCK_RATE_DEFAULT_BIND),
                panel: None,
            },
            StaticSlotFieldShape {
                name: "scrub_offset_seconds",
                shape: &StaticSlotShapeDescriptor::Value {
                    shape: static_clock_scrub_offset_shape(),
                },
                semantics: TRANSPORT_LEAF_SEMANTICS,
                role: SlotRole::Debug,
                default_bind: Some(CLOCK_SCRUB_DEFAULT_BIND),
                panel: None,
            },
        ],
    });

impl FieldSlot for ClockTransport {
    const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        STATIC_TRANSPORT_DESCRIPTOR;

    fn slot_field_shape() -> SlotShape {
        SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                crate::slot::shape::field_with_dataflow(
                    "play_state",
                    SlotShape::leaf(<PlayState as SlotValue>::value_shape()),
                    TRANSPORT_LEAF_SEMANTICS,
                    SlotRole::Debug,
                    Some(CLOCK_PLAY_STATE_DEFAULT_BIND),
                    None,
                ),
                crate::slot::shape::field_with_dataflow(
                    "rate",
                    SlotShape::leaf(clock_rate_shape()),
                    TRANSPORT_LEAF_SEMANTICS,
                    SlotRole::Debug,
                    Some(CLOCK_RATE_DEFAULT_BIND),
                    None,
                ),
                crate::slot::shape::field_with_dataflow(
                    "scrub_offset_seconds",
                    SlotShape::leaf(clock_scrub_offset_shape()),
                    TRANSPORT_LEAF_SEMANTICS,
                    SlotRole::Debug,
                    Some(CLOCK_SCRUB_DEFAULT_BIND),
                    None,
                ),
            ],
        }
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl FieldSlotMut for ClockTransport {
    fn slot_field_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Record(self)
    }
}

/// The transport record is a *named* shape: `lp::clock::Transport` identifies
/// it wherever the whole record travels as one value (a channel write, a
/// panel widget's emit) rather than field by field.
impl StaticSlotShape for ClockTransport {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(CLOCK_TRANSPORT_SHAPE_NAME);
    const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        STATIC_TRANSPORT_DESCRIPTOR;

    fn slot_shape() -> SlotShape {
        <Self as FieldSlot>::slot_field_shape()
    }

    fn shape_name() -> Option<&'static str> {
        Some(CLOCK_TRANSPORT_SHAPE_NAME)
    }
}

// --- ClockTransport: hand-rolled record value form, mirroring what
// `#[derive(SlotValue)]` emits for a named-field struct (the
// `PhasorConfig` precedent). The slots' revisions are not part of the value:
// a transport that rides a channel carries the three numbers, and whoever
// applies it stamps its own revision.

impl ToLpValue for ClockTransport {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some("ClockTransport".to_string()),
            fields: Vec::from([
                (
                    "play_state".to_string(),
                    self.play_state.value().to_lp_value(),
                ),
                ("rate".to_string(), self.rate.value().to_lp_value()),
                (
                    "scrub_offset_seconds".to_string(),
                    self.scrub_offset_seconds.value().to_lp_value(),
                ),
            ]),
        }
    }
}

impl FromLpValue for ClockTransport {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected ClockTransport struct"));
        };
        if name.as_deref() != Some("ClockTransport") || fields.len() != 3 {
            return Err(ValueRootError::new("expected ClockTransport struct"));
        }
        Ok(Self {
            play_state: ValueSlot::new(read_field(fields, 0, "play_state")?),
            rate: ValueSlot::new(read_field(fields, 1, "rate")?),
            scrub_offset_seconds: ValueSlot::new(read_field(fields, 2, "scrub_offset_seconds")?),
        })
    }
}

fn read_field<T: FromLpValue>(
    fields: &[(String, LpValue)],
    index: usize,
    name: &str,
) -> Result<T, ValueRootError> {
    match fields.get(index) {
        Some((field_name, value)) if field_name == name => T::from_lp_value(value),
        _ => Err(ValueRootError::new(alloc::format!(
            "expected ClockTransport.{name}"
        ))),
    }
}

impl SlotRecordAccess for ClockTransport {
    fn fields_revision(&self) -> Revision {
        Revision::default()
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(self.play_state.slot_field_data()),
            1 => Some(self.rate.slot_field_data()),
            2 => Some(self.scrub_offset_seconds.slot_field_data()),
            _ => None,
        }
    }
}

impl SlotRecordAccessMut for ClockTransport {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataAccessMut<'_>> {
        match index {
            0 => Some(SlotDataAccessMut::Value(&mut self.play_state)),
            1 => Some(SlotDataAccessMut::Value(&mut self.rate)),
            2 => Some(SlotDataAccessMut::Value(&mut self.scrub_offset_seconds)),
            _ => None,
        }
    }
}

impl SlotMapValueAccessMut for ClockTransport {
    fn slot_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Record(self)
    }
}

fn default_play_state() -> ValueSlot<PlayState> {
    ValueSlot::new(PlayState::Playing)
}

fn default_rate() -> ValueSlot<f32> {
    ValueSlot::new(1.0)
}

const fn static_clock_play_state_shape() -> StaticSlotValueShape {
    match <PlayState as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR {
        Some(shape) => shape,
        None => panic!("PlayState declares a static value shape"),
    }
}

/// Rate bounds are the tape transport's ¼×–8× span (plan
/// 2026-08-04-2355-clock-tape-hero, Q4). The step is deliberately `None`:
/// the transport widget owns the logarithmic mapping and its octave detents,
/// and a generic row editor only needs sane bounds.
fn clock_rate_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("lp::clock::Rate"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Slider {
            min: OrderedF32(0.25),
            max: OrderedF32(8.0),
            step: None,
        },
    }
}

const fn static_clock_rate_shape() -> StaticSlotValueShape {
    StaticSlotValueShape {
        id: SlotShapeId::from_static_name("lp::clock::Rate"),
        ty: StaticLpType::F32,
        meta: StaticSlotMeta::EMPTY,
        editor: StaticValueEditorHint::Slider {
            min: OrderedF32(0.25),
            max: OrderedF32(8.0),
            step: None,
        },
    }
}

fn clock_scrub_offset_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("lp::clock::ScrubOffsetSeconds"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Slider {
            min: OrderedF32(-10.0),
            max: OrderedF32(10.0),
            step: Some(OrderedF32(FRAME_SECONDS_60HZ)),
        },
    }
}

const fn static_clock_scrub_offset_shape() -> StaticSlotValueShape {
    StaticSlotValueShape {
        id: SlotShapeId::from_static_name("lp::clock::ScrubOffsetSeconds"),
        ty: StaticLpType::F32,
        meta: StaticSlotMeta::EMPTY,
        editor: StaticValueEditorHint::Slider {
            min: OrderedF32(-10.0),
            max: OrderedF32(10.0),
            step: Some(OrderedF32(FRAME_SECONDS_60HZ)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::well_known::{
        CLOCK_PLAY_STATE_CHANNEL, CLOCK_RATE_CHANNEL, CLOCK_SCRUB_CHANNEL,
    };

    #[test]
    fn clock_transport_fields_have_debug_role() {
        let SlotShape::Record { fields, .. } = ClockTransport::slot_field_shape() else {
            panic!("record shape");
        };
        assert_eq!(fields.len(), 3);
        for field in fields {
            assert_eq!(field.role, SlotRole::Debug);
            assert!(field.is_writable());
        }
    }

    /// Three leaves, three wires (P6/D20): every transport field declares its
    /// own `clock.*` default binding, and each endpoint names a channel the
    /// well-known registry teaches.
    #[test]
    fn every_transport_leaf_declares_its_own_clock_channel() {
        let SlotShape::Record { fields, .. } = ClockTransport::slot_field_shape() else {
            panic!("record shape");
        };
        let declared: Vec<(&str, Option<&str>)> = fields
            .iter()
            .map(|field| (field.name.as_str(), field.default_bind.as_deref()))
            .collect();
        assert_eq!(
            declared,
            vec![
                ("play_state", Some(CLOCK_PLAY_STATE_DEFAULT_BIND)),
                ("rate", Some(CLOCK_RATE_DEFAULT_BIND)),
                ("scrub_offset_seconds", Some(CLOCK_SCRUB_DEFAULT_BIND)),
            ]
        );
        for (endpoint, channel) in [
            (CLOCK_PLAY_STATE_DEFAULT_BIND, CLOCK_PLAY_STATE_CHANNEL),
            (CLOCK_RATE_DEFAULT_BIND, CLOCK_RATE_CHANNEL),
            (CLOCK_SCRUB_DEFAULT_BIND, CLOCK_SCRUB_CHANNEL),
        ] {
            assert_eq!(endpoint, alloc::format!("bus:{channel}"));
            assert!(
                crate::bus::well_known::well_known_channel(channel).is_some(),
                "{channel} must be a well-known channel"
            );
        }
        // The panel hint belongs to the RECORD (`ClockDef::transport`), never
        // to a leaf: one promoted record = one panel control.
        assert!(fields.iter().all(|field| field.panel.is_none()));
    }

    /// A play state is a state NOUN on the wire, and the editor hint is a
    /// discrete choice (the `FloatMode` precedent), not a bare boolean.
    #[test]
    fn play_state_is_a_string_leaf_with_a_choice_hint() {
        assert_eq!(PlayState::Playing.as_str(), "playing");
        assert_eq!(PlayState::Paused.as_str(), "paused");
        assert_eq!(PlayState::parse("paused"), Some(PlayState::Paused));
        assert_eq!(PlayState::parse("stopped"), None);
        assert_eq!(PlayState::default(), PlayState::Playing);
        assert!(PlayState::Playing.is_playing());
        assert_eq!(PlayState::Playing.toggled(), PlayState::Paused);
        assert_eq!(PlayState::Paused.toggled(), PlayState::Playing);

        let shape = <PlayState as SlotValue>::value_shape();
        assert_eq!(shape.ty, LpType::String);
        assert_eq!(
            shape.editor,
            ValueEditorHint::Dropdown {
                options: vec![
                    SlotEnumOption::new("playing", "Playing"),
                    SlotEnumOption::new("paused", "Paused"),
                ],
            }
        );
        assert_eq!(
            static_clock_play_state_shape()
                .editor
                .to_owned_editor_hint(),
            shape.editor
        );

        for state in PlayState::all() {
            let value = state.to_lp_value();
            assert_eq!(
                PlayState::from_lp_value(&value).expect("round trip"),
                *state
            );
        }
        assert!(PlayState::from_lp_value(&LpValue::Bool(true)).is_err());
    }

    #[test]
    fn static_and_dynamic_transport_shapes_agree() {
        let dynamic = <ClockTransport as FieldSlot>::slot_field_shape();
        let static_shape = <ClockTransport as StaticSlotShape>::STATIC_SLOT_SHAPE_DESCRIPTOR
            .expect("static descriptor");

        assert_eq!(static_shape.to_owned_shape(), dynamic);
        assert_eq!(
            <ClockTransport as StaticSlotShape>::SHAPE_ID,
            SlotShapeId::from_static_name(CLOCK_TRANSPORT_SHAPE_NAME)
        );
        assert_eq!(
            <ClockTransport as StaticSlotShape>::shape_name(),
            Some(CLOCK_TRANSPORT_SHAPE_NAME)
        );
    }

    #[test]
    fn clock_transport_round_trips_through_lp_value() {
        let transport = ClockTransport {
            play_state: ValueSlot::new(PlayState::Paused),
            rate: ValueSlot::new(2.5),
            scrub_offset_seconds: ValueSlot::new(-1.5),
        };

        let value = transport.to_lp_value();
        let back = ClockTransport::from_lp_value(&value).expect("round trip");

        assert_eq!(*back.play_state.value(), PlayState::Paused);
        assert_eq!(*back.rate.value(), 2.5);
        assert_eq!(*back.scrub_offset_seconds.value(), -1.5);
    }

    #[test]
    fn clock_transport_rejects_a_foreign_struct() {
        let wrong = LpValue::Struct {
            name: Some("Other".to_string()),
            fields: Vec::new(),
        };

        assert!(ClockTransport::from_lp_value(&wrong).is_err());
        assert!(ClockTransport::from_lp_value(&LpValue::F32(1.0)).is_err());
    }

    /// The tape transport's fader span (Q4): the widget maps ¼×–8×
    /// logarithmically, so the slot declares only the bounds.
    #[test]
    fn rate_hint_spans_the_transport_range() {
        assert_eq!(
            clock_rate_shape().editor,
            ValueEditorHint::Slider {
                min: OrderedF32(0.25),
                max: OrderedF32(8.0),
                step: None,
            }
        );
        assert_eq!(
            static_clock_rate_shape().editor.to_owned_editor_hint(),
            clock_rate_shape().editor
        );
    }
}

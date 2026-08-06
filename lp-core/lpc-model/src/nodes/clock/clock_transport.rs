use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    FieldSlot, FieldSlotMut, FromLpValue, LpType, LpValue, OrderedF32, Revision, SlotDataAccess,
    SlotDataAccessMut, SlotMapValueAccessMut, SlotMeta, SlotRecordAccess, SlotRecordAccessMut,
    SlotRole, SlotShape, SlotShapeId, SlotValueShape, StaticLpType, StaticSlotFieldShape,
    StaticSlotMeta, StaticSlotShape, StaticSlotShapeDescriptor, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

const FRAME_SECONDS_60HZ: f32 = 1.0 / 60.0;

/// Native shape name for [`ClockTransport`].
pub const CLOCK_TRANSPORT_SHAPE_NAME: &str = "lp::clock::Transport";

/// The project clock's transient transport: run/pause, rate, scrub offset.
///
/// The transport lives in authored node-def slot data so the UI can mutate it
/// through the same path as ordinary config. Its fields' slot role marks them
/// as `Debug`: writable but never persisted, since they are runtime transport
/// controls, not durable defaults.
///
/// The record is named ([`CLOCK_TRANSPORT_SHAPE_NAME`]) and carries an
/// [`LpValue`] struct form so the whole transport can ride a channel as one
/// value, the way [`crate::PhasorConfig`] does for a phasor's config.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockTransport {
    pub running: ValueSlot<bool>,
    pub rate: ValueSlot<f32>,
    pub scrub_offset_seconds: ValueSlot<f32>,
}

impl Default for ClockTransport {
    fn default() -> Self {
        Self {
            running: default_running(),
            rate: default_rate(),
            scrub_offset_seconds: ValueSlot::new(0.0),
        }
    }
}

/// Borrowed record descriptor shared by the [`FieldSlot`] mount and the named
/// [`StaticSlotShape`] identity — one declaration, so the two cannot drift.
const STATIC_TRANSPORT_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
    match <ValueSlot<bool> as FieldSlot>::STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR {
        Some(running_shape) => Some(&StaticSlotShapeDescriptor::Record {
            meta: StaticSlotMeta::EMPTY,
            fields: &[
                StaticSlotFieldShape {
                    name: "running",
                    shape: running_shape,
                    semantics: crate::SlotSemantics::local(),
                    role: SlotRole::Debug,
                    default_bind: None,
                    panel: None,
                },
                StaticSlotFieldShape {
                    name: "rate",
                    shape: &StaticSlotShapeDescriptor::Value {
                        shape: static_clock_rate_shape(),
                    },
                    semantics: crate::SlotSemantics::local(),
                    role: SlotRole::Debug,
                    default_bind: None,
                    panel: None,
                },
                StaticSlotFieldShape {
                    name: "scrub_offset_seconds",
                    shape: &StaticSlotShapeDescriptor::Value {
                        shape: static_clock_scrub_offset_shape(),
                    },
                    semantics: crate::SlotSemantics::local(),
                    role: SlotRole::Debug,
                    default_bind: None,
                    panel: None,
                },
            ],
        }),
        None => None,
    };

impl FieldSlot for ClockTransport {
    const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        STATIC_TRANSPORT_DESCRIPTOR;

    fn slot_field_shape() -> SlotShape {
        SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                crate::slot::shape::field_with_role(
                    "running",
                    ValueSlot::<bool>::slot_field_shape(),
                    SlotRole::Debug,
                ),
                crate::slot::shape::field_with_role(
                    "rate",
                    SlotShape::leaf(clock_rate_shape()),
                    SlotRole::Debug,
                ),
                crate::slot::shape::field_with_role(
                    "scrub_offset_seconds",
                    SlotShape::leaf(clock_scrub_offset_shape()),
                    SlotRole::Debug,
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
                ("running".to_string(), self.running.value().to_lp_value()),
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
            running: ValueSlot::new(read_field(fields, 0, "running")?),
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
            0 => Some(self.running.slot_field_data()),
            1 => Some(self.rate.slot_field_data()),
            2 => Some(self.scrub_offset_seconds.slot_field_data()),
            _ => None,
        }
    }
}

impl SlotRecordAccessMut for ClockTransport {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataAccessMut<'_>> {
        match index {
            0 => Some(SlotDataAccessMut::Value(&mut self.running)),
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

fn default_running() -> ValueSlot<bool> {
    ValueSlot::new(true)
}

fn default_rate() -> ValueSlot<f32> {
    ValueSlot::new(1.0)
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
            running: ValueSlot::new(false),
            rate: ValueSlot::new(2.5),
            scrub_offset_seconds: ValueSlot::new(-1.5),
        };

        let value = transport.to_lp_value();
        let back = ClockTransport::from_lp_value(&value).expect("round trip");

        assert_eq!(*back.running.value(), false);
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

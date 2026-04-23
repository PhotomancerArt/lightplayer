//! Shape (Scalar / Array / Struct) and Slot. See docs/design/lightplayer/quantity.md §6.

use crate::binding::Binding;
use crate::constraint::Constraint;
use crate::kind::Kind;
use crate::presentation::Presentation;
use crate::types::Name;
use crate::value_spec::{LoadCtx, ValueSpec};
use crate::{LpsType, LpsValue};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use lps_shared::StructMember;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    Scalar {
        kind: Kind,
        constraint: Constraint,
        default: ValueSpec,
    },
    Array {
        element: Box<Slot>,
        length: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<ValueSpec>,
    },
    Struct {
        fields: Vec<(Name, Slot)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<ValueSpec>,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Slot {
    pub shape: Shape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<Binding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub present: Option<Presentation>,
}

impl Slot {
    pub fn default_value(&self, ctx: &mut LoadCtx) -> LpsValue {
        match &self.shape {
            Shape::Scalar { default, .. } => default.materialize(ctx),
            Shape::Array {
                element,
                length,
                default,
            } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let mut elems = Vec::with_capacity(*length as usize);
                    for _ in 0..*length {
                        elems.push(element.default_value(ctx));
                    }
                    LpsValue::Array(elems.into_boxed_slice())
                }
            },
            Shape::Struct { fields, default } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let entries = fields
                        .iter()
                        .map(|(name, slot)| (name.0.clone(), slot.default_value(ctx)))
                        .collect();
                    LpsValue::Struct {
                        name: None,
                        fields: entries,
                    }
                }
            },
        }
    }

    pub fn storage(&self) -> LpsType {
        match &self.shape {
            Shape::Scalar { kind, .. } => kind.storage(),
            Shape::Array {
                element, length, ..
            } => LpsType::Array {
                element: Box::new(element.storage()),
                len: *length,
            },
            Shape::Struct { fields, .. } => LpsType::Struct {
                name: None,
                members: fields
                    .iter()
                    .map(|(name, slot)| StructMember {
                        name: Some(name.0.clone()),
                        ty: slot.storage(),
                    })
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar_amplitude_slot() -> Slot {
        Slot {
            shape: Shape::Scalar {
                kind: Kind::Amplitude,
                constraint: Constraint::Range {
                    min: 0.0,
                    max: 1.0,
                    step: None,
                },
                default: ValueSpec::Literal(LpsValue::F32(1.0)),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        }
    }

    #[test]
    fn scalar_default_value_is_literal() {
        let mut ctx = LoadCtx::default();
        match scalar_amplitude_slot().default_value(&mut ctx) {
            LpsValue::F32(v) => assert_eq!(v, 1.0),
            other => panic!("expected F32(1.0), got {other:?}"),
        }
    }

    #[test]
    fn array_with_no_default_derives_from_element() {
        let elem = scalar_amplitude_slot();
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(elem),
                length: 3,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let mut ctx = LoadCtx::default();
        match array_slot.default_value(&mut ctx) {
            LpsValue::Array(items) => {
                assert_eq!(items.len(), 3);
                for item in items.iter() {
                    match item {
                        LpsValue::F32(v) => assert_eq!(*v, 1.0),
                        other => panic!("expected F32, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn array_with_explicit_default_uses_override() {
        let elem = scalar_amplitude_slot();
        let preset: Vec<LpsValue> = alloc::vec![LpsValue::F32(0.2), LpsValue::F32(0.7)];
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(elem),
                length: 2,
                default: Some(ValueSpec::Literal(LpsValue::Array(
                    preset.into_boxed_slice(),
                ))),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let mut ctx = LoadCtx::default();
        match array_slot.default_value(&mut ctx) {
            LpsValue::Array(items) => {
                assert_eq!(items.len(), 2);
                match (&items[0], &items[1]) {
                    (LpsValue::F32(a), LpsValue::F32(b)) => {
                        assert_eq!(*a, 0.2);
                        assert_eq!(*b, 0.7);
                    }
                    other => panic!("expected two F32s, got {other:?}"),
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn struct_with_no_default_derives_from_fields() {
        let speed = (Name::parse("speed").unwrap(), scalar_amplitude_slot());
        let struct_slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let mut ctx = LoadCtx::default();
        match struct_slot.default_value(&mut ctx) {
            LpsValue::Struct { fields, .. } => {
                assert_eq!(fields.len(), 1);
                let (name, val) = &fields[0];
                assert_eq!(name, "speed");
                match val {
                    LpsValue::F32(v) => assert_eq!(*v, 1.0),
                    other => panic!("expected F32, got {other:?}"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn slot_storage_projection_scalar() {
        assert_eq!(scalar_amplitude_slot().storage(), LpsType::Float);
    }

    #[test]
    fn slot_storage_projection_array() {
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 4,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        match array_slot.storage() {
            LpsType::Array { element, len } => {
                assert_eq!(*element, LpsType::Float);
                assert_eq!(len, 4);
            }
            _ => panic!("expected Array storage"),
        }
    }

    #[test]
    fn slot_serde_round_trip_scalar() {
        let s = scalar_amplitude_slot();
        let json = serde_json::to_string(&s).unwrap();
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn slot_serde_omits_none_overrides_on_composed() {
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 2,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let json = serde_json::to_string(&array_slot).unwrap();
        assert!(!json.contains("\"default\":null"));
    }

    #[test]
    fn slot_serde_round_trips_recursive() {
        let speed = (Name::parse("speed").unwrap(), scalar_amplitude_slot());
        let struct_slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let json = serde_json::to_string(&struct_slot).unwrap();
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(struct_slot, back);
    }
}

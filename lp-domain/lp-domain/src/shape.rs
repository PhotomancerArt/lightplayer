//! [`Shape`]: the **structural** skeleton of a value (what WGSL/GLSL can
//! represent); [`Slot`]: a [`Shape`] plus **metadata** and wiring hints.
//!
//! Together they are the **composition** layer of the Quantity model
//! (`docs/design/lightplayer/quantity.md` §1, §2, and §6). `Shape` is *only*
//! `Scalar | Array | Struct` (no tuples or sum types) so every slot’s storage
//! projects cleanly to a [`crate::LpsType`] and GPU layouts (`quantity.md` §6,
//! “Why no tuples”).
//!
//! **Defaults (M2, “Q15 Option A”):** [`Shape::Scalar`] carries a **mandatory**
//! [`ValueSpec`][`crate::value_spec::ValueSpec`]. [`Shape::Array`] and
//! [`Shape::Struct`] carry `default: Option<ValueSpec>`; if `None`, the
//! default is **derived** at materialize time from child slots (arrays:
//! N copies; structs: one field per child). If `Some`, that aggregate spec
//! wins. See `quantity.md` §6 “Defaults for compositions” and
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (Q15). A
//! [`Slot`] has **no** separate top-level `default` field: defaults are entirely
//! expressed through [`Shape`].
//!
//! [`Shape::Struct`]’s `fields` are a **vector** to preserve TOML order, std430
//! layout, and panel field order (`quantity.md` §6).

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

/// The **recursive** shape of a slot: scalar, fixed-length array, or ordered struct.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    /// One leaf value: [`Kind`], [`Constraint`], and a **required** default
    /// [`ValueSpec`][`crate::value_spec::ValueSpec`] (there is nothing to
    /// derive a default from, `quantity.md` §6).
    Scalar {
        kind: Kind,
        constraint: Constraint,
        default: ValueSpec,
    },
    /// A fixed `length` of `element` slots; optional **aggregate** default
    /// (see module docs). `None` ⇒ N-element array from
    /// [`Slot::default_value`].
    Array {
        element: Box<Slot>,
        length: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<ValueSpec>,
    },
    /// Ordered struct fields. Optional aggregate default: `None` ⇒ struct
    /// map from each field’s default (`quantity.md` §6).
    Struct {
        fields: Vec<(Name, Slot)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<ValueSpec>,
    },
}

/// A **slot declaration**: a [`Shape`] plus optional human and runtime-facing
/// metadata.
///
/// `label` / `description` are for author-facing UI. `bind` and `present`
/// connect to the bus and widget hints; when `present` is `None`, tools use
/// [`Kind::default_presentation`][`crate::kind::Kind::default_presentation`]
/// (`docs/design/lightplayer/quantity.md` §8–9). The slot’s **value default**
/// is fully determined by the nested [`Shape`], not a separate field (Q15, see
/// module `//!` above).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Slot {
    /// The structural and default-bearing part of the slot.
    pub shape: Shape,
    /// Short user-facing name (optional; falls back to kind defaults in UI, `quantity.md` §6 sketch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Longer description (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// If set, **overrides** [`Kind::default_bind`][`crate::kind::Kind::default_bind`]
    /// for input-side bus wiring (`docs/design/lightplayer/quantity.md` §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<Binding>,
    /// If set, **overrides** [`Kind::default_presentation`][`crate::kind::Kind::default_presentation`]
    /// for UI (`docs/design/lightplayer/quantity.md` §9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub present: Option<Presentation>,
}

impl Slot {
    /// Materialize the **default** for this slot: for [`Shape::Scalar`],
    /// [`ValueSpec::materialize`][`crate::value_spec::ValueSpec::materialize`]
    /// on the scalar’s `default`. For array/struct, if `default` is `Some`, use
    /// that; otherwise build `Array` of `length` / `Struct` of field name →
    /// child default, per `docs/design/lightplayer/quantity.md` §6 “Defaults
    /// for compositions”.
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

    /// Structural type for GPU and serializers: for scalars,
    /// [`Kind::storage`](crate::kind::Kind::storage) for the leaf kind; for arrays, element type
    /// with length; for structs, ordered members (`quantity.md` §2 table and §6
    /// `storage()` sketch).
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

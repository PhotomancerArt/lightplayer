//! Native fluid emitter value shape.
//!
//! `FluidEmitter` is a complete slot value leaf. Collections of emitters should
//! be represented as `MapSlot<u32, FluidEmitter>` or equivalent map-shaped slot
//! data so each emitter has stable identity at the slot layer.

use crate::{
    FieldSlot, FromLpValue, LpType, LpValue, ModelStructMember, SlotDataAccess, SlotMeta,
    SlotShape, SlotShapeId, SlotValue, SlotValueAccess, SlotValueShape, StaticSlotShape, ToLpValue,
    ValueEditorHint, ValueRootError,
};
use alloc::string::String;
use alloc::vec;
use serde::{Deserialize, Serialize};

/// Native shape name used by authored shader slot defs.
pub const FLUID_EMITTER_SHAPE_NAME: &str = "lp::fluid::Emitter";

/// One fluid emission source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FluidEmitter {
    pub id: u32,
    pub pos: [f32; 2],
    pub dir: [f32; 2],
    pub radius: f32,
    pub color: [f32; 3],
    pub velocity: f32,
    pub intensity: f32,
}

impl FluidEmitter {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            pos: [0.5, 0.5],
            dir: [1.0, 0.0],
            radius: 0.05,
            color: [1.0, 1.0, 1.0],
            velocity: 0.0,
            intensity: 1.0,
        }
    }
}

impl ToLpValue for FluidEmitter {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some(String::from("FluidEmitter")),
            fields: vec![
                (String::from("id"), LpValue::U32(self.id)),
                (String::from("pos"), LpValue::Vec2(self.pos)),
                (String::from("dir"), LpValue::Vec2(self.dir)),
                (String::from("radius"), LpValue::F32(self.radius)),
                (String::from("color"), LpValue::Vec3(self.color)),
                (String::from("velocity"), LpValue::F32(self.velocity)),
                (String::from("intensity"), LpValue::F32(self.intensity)),
            ],
        }
    }
}

impl FromLpValue for FluidEmitter {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { fields, .. } = value else {
            return Err(ValueRootError::new("expected FluidEmitter struct"));
        };

        Ok(Self {
            id: expect_u32(fields, "id")?,
            pos: expect_vec2(fields, "pos")?,
            dir: expect_vec2(fields, "dir")?,
            radius: expect_f32(fields, "radius")?,
            color: expect_vec3(fields, "color")?,
            velocity: expect_f32(fields, "velocity")?,
            intensity: expect_f32(fields, "intensity")?,
        })
    }
}

impl SlotValue for FluidEmitter {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(FLUID_EMITTER_SHAPE_NAME);

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <Self as SlotValue>::SHAPE_ID,
            ty: fluid_emitter_lp_type(),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

impl SlotValueAccess for FluidEmitter {
    fn changed_at(&self) -> crate::Revision {
        crate::current_revision()
    }

    fn value(&self) -> LpValue {
        self.to_lp_value()
    }
}

impl FieldSlot for FluidEmitter {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl StaticSlotShape for FluidEmitter {
    const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;

    fn slot_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn shape_name() -> Option<&'static str> {
        Some(FLUID_EMITTER_SHAPE_NAME)
    }
}

pub fn fluid_emitter_lp_type() -> LpType {
    LpType::Struct {
        name: Some(String::from("FluidEmitter")),
        fields: vec![
            ModelStructMember {
                name: String::from("id"),
                ty: LpType::U32,
            },
            ModelStructMember {
                name: String::from("pos"),
                ty: LpType::Vec2,
            },
            ModelStructMember {
                name: String::from("dir"),
                ty: LpType::Vec2,
            },
            ModelStructMember {
                name: String::from("radius"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("color"),
                ty: LpType::Vec3,
            },
            ModelStructMember {
                name: String::from("velocity"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("intensity"),
                ty: LpType::F32,
            },
        ],
    }
}

fn field<'a>(fields: &'a [(String, LpValue)], name: &str) -> Result<&'a LpValue, ValueRootError> {
    fields
        .iter()
        .find_map(|(field_name, value)| (field_name == name).then_some(value))
        .ok_or_else(|| ValueRootError::new(alloc::format!("missing FluidEmitter.{name}")))
}

fn expect_u32(fields: &[(String, LpValue)], name: &str) -> Result<u32, ValueRootError> {
    match field(fields, name)? {
        LpValue::U32(value) => Ok(*value),
        other => Err(ValueRootError::new(alloc::format!(
            "expected FluidEmitter.{name} u32, got {other:?}"
        ))),
    }
}

fn expect_f32(fields: &[(String, LpValue)], name: &str) -> Result<f32, ValueRootError> {
    match field(fields, name)? {
        LpValue::F32(value) => Ok(*value),
        other => Err(ValueRootError::new(alloc::format!(
            "expected FluidEmitter.{name} f32, got {other:?}"
        ))),
    }
}

fn expect_vec2(fields: &[(String, LpValue)], name: &str) -> Result<[f32; 2], ValueRootError> {
    match field(fields, name)? {
        LpValue::Vec2(value) => Ok(*value),
        other => Err(ValueRootError::new(alloc::format!(
            "expected FluidEmitter.{name} vec2, got {other:?}"
        ))),
    }
}

fn expect_vec3(fields: &[(String, LpValue)], name: &str) -> Result<[f32; 3], ValueRootError> {
    match field(fields, name)? {
        LpValue::Vec3(value) => Ok(*value),
        other => Err(ValueRootError::new(alloc::format!(
            "expected FluidEmitter.{name} vec3, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlotShapeRegistry;

    #[test]
    fn fluid_emitter_round_trips_through_lp_value() {
        let emitter = FluidEmitter {
            id: 7,
            pos: [0.25, 0.75],
            dir: [0.0, 1.0],
            radius: 0.1,
            color: [1.0, 0.5, 0.25],
            velocity: 0.2,
            intensity: 0.8,
        };

        assert_eq!(
            FluidEmitter::from_lp_value(&emitter.to_lp_value()).unwrap(),
            emitter
        );
    }

    #[test]
    fn fluid_emitter_registers_native_shape_name() {
        let mut registry = SlotShapeRegistry::default();

        FluidEmitter::ensure_registered(&mut registry).expect("registered");

        assert_eq!(
            registry.id_for_name(FLUID_EMITTER_SHAPE_NAME),
            Some(<FluidEmitter as SlotValue>::SHAPE_ID)
        );
        assert!(registry.get_by_name(FLUID_EMITTER_SHAPE_NAME).is_some());
    }
}

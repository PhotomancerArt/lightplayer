//! ValueSpec: author-time defaults (literal values + opaque-handle recipes).
//! See docs/design/lightplayer/quantity.md §7.

use crate::LpsValue;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_materializes_to_itself() {
        let mut ctx = LoadCtx::default();
        let spec = ValueSpec::Literal(LpsValue::F32(0.5));
        match spec.materialize(&mut ctx) {
            LpsValue::F32(v) => assert_eq!(v, 0.5),
            other => panic!("expected F32(0.5), got {other:?}"),
        }
    }

    #[test]
    fn texture_black_materializes_to_handle_zero() {
        let mut ctx = LoadCtx::default();
        let spec = ValueSpec::Texture(TextureSpec::Black);
        let v = spec.materialize(&mut ctx);
        match v {
            LpsValue::Struct { fields, .. } => {
                let handle = fields
                    .iter()
                    .find(|(n, _)| n == "handle")
                    .expect("handle field");
                match &handle.1 {
                    LpsValue::I32(h) => assert_eq!(*h, 0),
                    _ => panic!("handle must be I32"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }
}

/// Loader-side context. Phase 5 ships a stub; M3 fills with a real
/// texture handle allocator and asset cache.
#[derive(Default)]
pub struct LoadCtx {
    pub next_texture_handle: i32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum LpsValueWire {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]),
    Mat3x3([[f32; 3]; 3]),
    Mat4x4([[f32; 4]; 4]),
    Array(Vec<LpsValueWire>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpsValueWire)>,
    },
}

impl From<&LpsValue> for LpsValueWire {
    fn from(v: &LpsValue) -> Self {
        match v {
            LpsValue::I32(x) => LpsValueWire::I32(*x),
            LpsValue::U32(x) => LpsValueWire::U32(*x),
            LpsValue::F32(x) => LpsValueWire::F32(*x),
            LpsValue::Bool(x) => LpsValueWire::Bool(*x),
            LpsValue::Vec2(x) => LpsValueWire::Vec2(*x),
            LpsValue::Vec3(x) => LpsValueWire::Vec3(*x),
            LpsValue::Vec4(x) => LpsValueWire::Vec4(*x),
            LpsValue::IVec2(x) => LpsValueWire::IVec2(*x),
            LpsValue::IVec3(x) => LpsValueWire::IVec3(*x),
            LpsValue::IVec4(x) => LpsValueWire::IVec4(*x),
            LpsValue::UVec2(x) => LpsValueWire::UVec2(*x),
            LpsValue::UVec3(x) => LpsValueWire::UVec3(*x),
            LpsValue::UVec4(x) => LpsValueWire::UVec4(*x),
            LpsValue::BVec2(x) => LpsValueWire::BVec2(*x),
            LpsValue::BVec3(x) => LpsValueWire::BVec3(*x),
            LpsValue::BVec4(x) => LpsValueWire::BVec4(*x),
            LpsValue::Mat2x2(x) => LpsValueWire::Mat2x2(*x),
            LpsValue::Mat3x3(x) => LpsValueWire::Mat3x3(*x),
            LpsValue::Mat4x4(x) => LpsValueWire::Mat4x4(*x),
            LpsValue::Array(a) => LpsValueWire::Array(a.iter().map(LpsValueWire::from).collect()),
            LpsValue::Struct { name, fields } => LpsValueWire::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), LpsValueWire::from(v)))
                    .collect(),
            },
        }
    }
}

impl From<LpsValueWire> for LpsValue {
    fn from(w: LpsValueWire) -> Self {
        match w {
            LpsValueWire::I32(x) => LpsValue::I32(x),
            LpsValueWire::U32(x) => LpsValue::U32(x),
            LpsValueWire::F32(x) => LpsValue::F32(x),
            LpsValueWire::Bool(x) => LpsValue::Bool(x),
            LpsValueWire::Vec2(x) => LpsValue::Vec2(x),
            LpsValueWire::Vec3(x) => LpsValue::Vec3(x),
            LpsValueWire::Vec4(x) => LpsValue::Vec4(x),
            LpsValueWire::IVec2(x) => LpsValue::IVec2(x),
            LpsValueWire::IVec3(x) => LpsValue::IVec3(x),
            LpsValueWire::IVec4(x) => LpsValue::IVec4(x),
            LpsValueWire::UVec2(x) => LpsValue::UVec2(x),
            LpsValueWire::UVec3(x) => LpsValue::UVec3(x),
            LpsValueWire::UVec4(x) => LpsValue::UVec4(x),
            LpsValueWire::BVec2(x) => LpsValue::BVec2(x),
            LpsValueWire::BVec3(x) => LpsValue::BVec3(x),
            LpsValueWire::BVec4(x) => LpsValue::BVec4(x),
            LpsValueWire::Mat2x2(x) => LpsValue::Mat2x2(x),
            LpsValueWire::Mat3x3(x) => LpsValue::Mat3x3(x),
            LpsValueWire::Mat4x4(x) => LpsValue::Mat4x4(x),
            LpsValueWire::Array(items) => LpsValue::Array(
                items
                    .into_iter()
                    .map(LpsValue::from)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            LpsValueWire::Struct { name, fields } => LpsValue::Struct {
                name,
                fields: fields
                    .into_iter()
                    .map(|(k, v)| (k, LpsValue::from(v)))
                    .collect(),
            },
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum ValueSpecWire {
    Literal(LpsValueWire),
    Texture(TextureSpec),
}

impl From<&ValueSpec> for ValueSpecWire {
    fn from(s: &ValueSpec) -> Self {
        match s {
            ValueSpec::Literal(v) => ValueSpecWire::Literal(LpsValueWire::from(v)),
            ValueSpec::Texture(t) => ValueSpecWire::Texture(t.clone()),
        }
    }
}

impl From<ValueSpecWire> for ValueSpec {
    fn from(w: ValueSpecWire) -> Self {
        match w {
            ValueSpecWire::Literal(v) => ValueSpec::Literal(LpsValue::from(v)),
            ValueSpecWire::Texture(t) => ValueSpec::Texture(t),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ValueSpec {
    Literal(LpsValue),
    Texture(TextureSpec),
}

impl serde::Serialize for ValueSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ValueSpecWire::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ValueSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ValueSpecWire::deserialize(deserializer).map(ValueSpec::from)
    }
}

impl PartialEq for ValueSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(a), Self::Literal(b)) => a.eq(b),
            (Self::Texture(a), Self::Texture(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextureSpec {
    Black,
}

impl ValueSpec {
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue {
        match self {
            Self::Literal(v) => v.clone(),
            Self::Texture(spec) => spec.materialize(ctx),
        }
    }
}

impl TextureSpec {
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue {
        match self {
            Self::Black => texture_handle_value(ctx, 0, 1, 1),
        }
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ValueSpec {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <ValueSpecWire as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <ValueSpecWire as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <ValueSpecWire as schemars::JsonSchema>::json_schema(generator)
    }
}

fn texture_handle_value(ctx: &mut LoadCtx, format: i32, width: i32, height: i32) -> LpsValue {
    let handle = ctx.next_texture_handle;
    LpsValue::Struct {
        name: None,
        fields: alloc::vec![
            (String::from("format"), LpsValue::I32(format)),
            (String::from("width"), LpsValue::I32(width)),
            (String::from("height"), LpsValue::I32(height)),
            (String::from("handle"), LpsValue::I32(handle)),
        ],
    }
}

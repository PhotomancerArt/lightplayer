use crate::source::ShaderDef;
use lpc_model::{
    __private::Box,
    LpType, LpValue, ModelStructMember, Revision, SlotAccess, SlotData, SlotDataAccess,
    SlotDataMutAccess, SlotMutAccess, SlotName, SlotOptionDyn, SlotRecord, SlotRecordAccess,
    SlotRecordMutAccess, SlotShape, SlotShapeId, WithRevision, current_revision,
    slot::shape::{field, option, record, value},
};

pub struct ShaderNode {
    shape_id: SlotShapeId,
    param_names: Vec<SlotName>,
    params: SlotRecord,
    compile_error: SlotOptionDyn,
}

impl ShaderNode {
    pub const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("engine.shader_node");

    pub fn from_def(def: &ShaderDef) -> Self {
        Self::from_def_with_shape_id(def, Self::SHAPE_ID)
    }

    pub fn from_def_with_shape_id(def: &ShaderDef, shape_id: SlotShapeId) -> Self {
        let param_names = def
            .param_defs
            .entries
            .keys()
            .map(|name| SlotName::parse(name).expect("shader param name"))
            .collect::<Vec<_>>();
        let params = def
            .param_defs
            .entries
            .values()
            .map(|param_def| {
                SlotData::Value(WithRevision::new(
                    current_revision(),
                    param_def.default_value(),
                ))
            })
            .collect::<Vec<_>>();

        Self {
            shape_id,
            param_names,
            params: SlotRecord::new(params),
            compile_error: SlotOptionDyn::some_with_version(
                current_revision(),
                SlotData::Value(WithRevision::new(
                    current_revision(),
                    LpValue::String(String::from("initial compile warning")),
                )),
            ),
        }
    }

    pub fn shape(&self) -> SlotShape {
        record(vec![
            field(
                "params",
                record(
                    self.param_names
                        .iter()
                        .zip(self.params.fields.iter())
                        .map(|(name, data)| field(name.as_str(), value(lp_type_for_data(data))))
                        .collect(),
                ),
            ),
            field("compile_error", option(value(LpType::String))),
        ])
    }

    pub fn set_param(&mut self, name: &str, value: f32) {
        self.set_param_value(name, LpValue::F32(value));
    }

    pub fn set_param_vec3(&mut self, name: &str, value: [f32; 3]) {
        self.set_param_value(name, LpValue::Vec3(value));
    }

    fn set_param_value(&mut self, name: &str, value: LpValue) {
        let index = self.param_index(name);
        let Some(SlotData::Value(param)) = self.params.fields.get_mut(index) else {
            panic!("shader param exists");
        };
        param.set(current_revision(), value);
    }

    pub fn remove_param(&mut self, name: &str) {
        let index = self.param_index(name);
        self.param_names.remove(index);
        self.params.fields.remove(index);
        self.params.fields_revision = current_revision();
    }

    pub fn param_revision(&self, name: &str) -> Option<Revision> {
        let index = self
            .param_names
            .iter()
            .position(|param_name| param_name.as_str() == name)?;
        let SlotData::Value(value) = self.params.fields.get(index)? else {
            return None;
        };
        Some(value.changed_at())
    }

    pub fn param_lp_type(&self, name: &str) -> Option<LpType> {
        let index = self
            .param_names
            .iter()
            .position(|param_name| param_name.as_str() == name)?;
        self.params.fields.get(index).map(lp_type_for_data)
    }

    pub fn clear_compile_error(&mut self) {
        self.compile_error = SlotOptionDyn::none();
    }

    fn param_index(&self, name: &str) -> usize {
        self.param_names
            .iter()
            .position(|param_name| param_name.as_str() == name)
            .expect("shader param exists")
    }
}

impl SlotAccess for ShaderNode {
    fn shape_id(&self) -> SlotShapeId {
        self.shape_id
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
        self
    }
}

impl SlotRecordAccess for ShaderNode {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Record(&self.params)),
            1 => Some(SlotDataAccess::Option(&self.compile_error)),
            _ => None,
        }
    }
}

impl SlotMutAccess for ShaderNode {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Record(self)
    }
}

impl SlotRecordMutAccess for ShaderNode {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        match index {
            0 => Some(SlotDataMutAccess::Record(&mut self.params)),
            1 => Some(SlotDataMutAccess::Option(&mut self.compile_error)),
            _ => None,
        }
    }
}

fn lp_type_for_data(data: &SlotData) -> LpType {
    let SlotData::Value(value) = data else {
        panic!("shader param value must be a value slot");
    };
    lp_type_for_value(value.value())
}

fn lp_type_for_value(value: &LpValue) -> LpType {
    match value {
        LpValue::Unset => panic!("unset shader param values need an explicit type"),
        LpValue::String(_) => LpType::String,
        LpValue::I32(_) => LpType::I32,
        LpValue::U32(_) => LpType::U32,
        LpValue::F32(_) => LpType::F32,
        LpValue::Bool(_) => LpType::Bool,
        LpValue::Vec2(_) => LpType::Vec2,
        LpValue::Vec3(_) => LpType::Vec3,
        LpValue::Vec4(_) => LpType::Vec4,
        LpValue::IVec2(_) => LpType::IVec2,
        LpValue::IVec3(_) => LpType::IVec3,
        LpValue::IVec4(_) => LpType::IVec4,
        LpValue::UVec2(_) => LpType::UVec2,
        LpValue::UVec3(_) => LpType::UVec3,
        LpValue::UVec4(_) => LpType::UVec4,
        LpValue::BVec2(_) => LpType::BVec2,
        LpValue::BVec3(_) => LpType::BVec3,
        LpValue::BVec4(_) => LpType::BVec4,
        LpValue::Mat2x2(_) => LpType::Mat2x2,
        LpValue::Mat3x3(_) => LpType::Mat3x3,
        LpValue::Mat4x4(_) => LpType::Mat4x4,
        LpValue::Array(values) => {
            let Some(first) = values.first() else {
                panic!("empty shader param arrays need an explicit type");
            };
            LpType::Array(Box::new(lp_type_for_value(first)), values.len())
        }
        LpValue::Struct { name, fields } => LpType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| ModelStructMember {
                    name: name.clone(),
                    ty: lp_type_for_value(value),
                })
                .collect(),
        },
        LpValue::Enum { .. } => panic!("shader param enum values need an explicit type"),
        LpValue::Resource(_) => LpType::Resource,
        LpValue::Product(product) => match product {
            lpc_model::ProductRef::Visual(_) => LpType::Product(lpc_model::ProductKind::Visual),
            lpc_model::ProductRef::Control(_) => LpType::Product(lpc_model::ProductKind::Control),
        },
    }
}

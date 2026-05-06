use lpc_model::{
    FrameId, ModelStructMember, ModelType, ModelValue, SlotAccess, SlotData, SlotDataAccess,
    SlotName, SlotOptionDyn, SlotRecord, SlotRecordAccess, SlotShape, SlotShapeId, Versioned,
    current_state_version,
};

use crate::model::{field, option, record, value};
use crate::source::ShaderDef;

pub struct ShaderNode {
    param_names: Vec<SlotName>,
    params: SlotRecord,
    compile_error: SlotOptionDyn,
}

impl ShaderNode {
    pub const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("engine.shader_node");

    pub fn from_def(def: &ShaderDef) -> Self {
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
                SlotData::Value(Versioned::new(
                    current_state_version(),
                    param_def.default_value(),
                ))
            })
            .collect::<Vec<_>>();

        Self {
            param_names,
            params: SlotRecord::new(params),
            compile_error: SlotOptionDyn::some_with_version(
                current_state_version(),
                SlotData::Value(Versioned::new(
                    current_state_version(),
                    ModelValue::String(String::from("initial compile warning")),
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
                        .map(|(name, data)| field(name.as_str(), value(model_type_for_data(data))))
                        .collect(),
                ),
            ),
            field("compile_error", option(value(ModelType::String))),
        ])
    }

    pub fn set_param(&mut self, name: &str, value: f32) {
        self.set_param_value(name, ModelValue::F32(value));
    }

    pub fn set_param_vec3(&mut self, name: &str, value: [f32; 3]) {
        self.set_param_value(name, ModelValue::Vec3(value));
    }

    fn set_param_value(&mut self, name: &str, value: ModelValue) {
        let index = self.param_index(name);
        let Some(SlotData::Value(param)) = self.params.fields.get_mut(index) else {
            panic!("shader param exists");
        };
        param.set(current_state_version(), value);
    }

    pub fn remove_param(&mut self, name: &str) {
        let index = self.param_index(name);
        self.param_names.remove(index);
        self.params.fields.remove(index);
        self.params.fields_changed_frame = current_state_version();
    }

    pub fn param_changed_frame(&self, name: &str) -> Option<FrameId> {
        let index = self
            .param_names
            .iter()
            .position(|param_name| param_name.as_str() == name)?;
        let SlotData::Value(value) = self.params.fields.get(index)? else {
            return None;
        };
        Some(value.changed_frame())
    }

    pub fn param_model_type(&self, name: &str) -> Option<ModelType> {
        let index = self
            .param_names
            .iter()
            .position(|param_name| param_name.as_str() == name)?;
        self.params.fields.get(index).map(model_type_for_data)
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
        Self::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
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

fn model_type_for_data(data: &SlotData) -> ModelType {
    let SlotData::Value(value) = data else {
        panic!("shader param value must be a value slot");
    };
    model_type_for_value(value.value())
}

fn model_type_for_value(value: &ModelValue) -> ModelType {
    match value {
        ModelValue::String(_) => ModelType::String,
        ModelValue::I32(_) => ModelType::I32,
        ModelValue::U32(_) => ModelType::U32,
        ModelValue::F32(_) => ModelType::F32,
        ModelValue::Bool(_) => ModelType::Bool,
        ModelValue::Vec2(_) => ModelType::Vec2,
        ModelValue::Vec3(_) => ModelType::Vec3,
        ModelValue::Vec4(_) => ModelType::Vec4,
        ModelValue::IVec2(_) => ModelType::IVec2,
        ModelValue::IVec3(_) => ModelType::IVec3,
        ModelValue::IVec4(_) => ModelType::IVec4,
        ModelValue::UVec2(_) => ModelType::UVec2,
        ModelValue::UVec3(_) => ModelType::UVec3,
        ModelValue::UVec4(_) => ModelType::UVec4,
        ModelValue::BVec2(_) => ModelType::BVec2,
        ModelValue::BVec3(_) => ModelType::BVec3,
        ModelValue::BVec4(_) => ModelType::BVec4,
        ModelValue::Mat2x2(_) => ModelType::Mat2x2,
        ModelValue::Mat3x3(_) => ModelType::Mat3x3,
        ModelValue::Mat4x4(_) => ModelType::Mat4x4,
        ModelValue::Array(values) => {
            let Some(first) = values.first() else {
                panic!("empty shader param arrays need an explicit type");
            };
            ModelType::Array(Box::new(model_type_for_value(first)), values.len())
        }
        ModelValue::Struct { name, fields } => ModelType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| ModelStructMember {
                    name: name.clone(),
                    ty: model_type_for_value(value),
                })
                .collect(),
        },
        ModelValue::Resource(_) => ModelType::Resource,
    }
}

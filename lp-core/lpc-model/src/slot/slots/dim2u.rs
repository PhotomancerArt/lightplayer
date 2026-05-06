use crate::{
    FieldSlot, FrameId, FromModelValue, ModelStructMember, ModelType, ModelValue, SlotDataAccess,
    SlotEditorHint, SlotLeaf, SlotLeafError, SlotLeafId, SlotMeta, SlotShape, SlotValueAccess,
    SlotValueShape, ToModelValue, Versioned, current_state_version,
};
use alloc::string::String;
use alloc::vec;

/// Width/height dimensions in unsigned integer pixels or cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dim2u {
    pub width: u32,
    pub height: u32,
}

/// Versioned unsigned 2D dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dim2uSlot {
    inner: Versioned<Dim2u>,
}

impl Dim2uSlot {
    pub fn new(value: Dim2u) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: Dim2u) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: Dim2u) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &Dim2u {
        self.inner.value()
    }
}

impl SlotValueAccess for Dim2uSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        self.inner.value().to_model_value()
    }
}

impl FieldSlot for Dim2uSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(dim2u_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToModelValue for Dim2u {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Struct {
            name: Some(String::from("Dim2u")),
            fields: vec![
                (String::from("width"), ModelValue::U32(self.width)),
                (String::from("height"), ModelValue::U32(self.height)),
            ],
        }
    }
}

impl FromModelValue for Dim2u {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        let ModelValue::Struct { name, fields } = value else {
            return Err(SlotLeafError::new("expected Dim2u struct"));
        };
        if name.as_deref() != Some("Dim2u") || fields.len() != 2 {
            return Err(SlotLeafError::new("expected Dim2u struct"));
        }
        let width = match &fields[0] {
            (name, ModelValue::U32(value)) if name == "width" => *value,
            _ => return Err(SlotLeafError::new("expected Dim2u.width")),
        };
        let height = match &fields[1] {
            (name, ModelValue::U32(value)) if name == "height" => *value,
            _ => return Err(SlotLeafError::new("expected Dim2u.height")),
        };
        Ok(Self { width, height })
    }
}

impl SlotLeaf for Dim2u {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.dim2u");

    fn value_shape() -> SlotValueShape {
        dim2u_shape()
    }
}

pub fn dim2u_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.dim2u"),
        ty: dim2u_model_type(),
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Dimensions,
    }
}

fn dim2u_model_type() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("Dim2u")),
        fields: vec![
            ModelStructMember {
                name: String::from("width"),
                ty: ModelType::U32,
            },
            ModelStructMember {
                name: String::from("height"),
                ty: ModelType::U32,
            },
        ],
    }
}

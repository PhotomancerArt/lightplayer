use crate::{
    FieldSlot, FrameId, FromLpValue, LpType, LpValue, SlotDataAccess, SlotEditorHint,
    SlotEnumOption, SlotLeaf, SlotLeafError, SlotLeafId, SlotMeta, SlotShape, SlotValueAccess,
    SlotValueShape, ToLpValue, Versioned, current_state_version,
};
use alloc::string::{String, ToString};
use alloc::vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// RGB channel order for fixture/output color packing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorOrderValue {
    Rgb,
    Grb,
    Rbg,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrderValue {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Grb => "grb",
            Self::Rbg => "rbg",
            Self::Gbr => "gbr",
            Self::Brg => "brg",
            Self::Bgr => "bgr",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rgb" => Some(Self::Rgb),
            "grb" => Some(Self::Grb),
            "rbg" => Some(Self::Rbg),
            "gbr" => Some(Self::Gbr),
            "brg" => Some(Self::Brg),
            "bgr" => Some(Self::Bgr),
            _ => None,
        }
    }
}

impl Serialize for ColorOrderValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ColorOrderValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| {
            serde::de::Error::custom(alloc::format!("unknown color order {value:?}"))
        })
    }
}

/// Versioned RGB channel order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorOrderSlot {
    inner: Versioned<ColorOrderValue>,
}

impl ColorOrderSlot {
    pub fn new(value: ColorOrderValue) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: ColorOrderValue) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: ColorOrderValue) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &ColorOrderValue {
        self.inner.value()
    }
}

impl SlotValueAccess for ColorOrderSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl Serialize for ColorOrderSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ColorOrderSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(ColorOrderValue::deserialize(deserializer)?))
    }
}

impl FieldSlot for ColorOrderSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(color_order_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToLpValue for ColorOrderValue {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for ColorOrderValue {
    fn from_lp_value(value: LpValue) -> Result<Self, SlotLeafError> {
        match value {
            LpValue::String(value) => Self::parse(&value)
                .ok_or_else(|| SlotLeafError::new("expected RGB color order value")),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotLeaf for ColorOrderValue {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.color_order");

    fn value_shape() -> SlotValueShape {
        color_order_shape()
    }
}

pub fn color_order_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.color_order"),
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Dropdown {
            options: vec![
                SlotEnumOption::new("rgb", "RGB"),
                SlotEnumOption::new("grb", "GRB"),
                SlotEnumOption::new("rbg", "RBG"),
                SlotEnumOption::new("gbr", "GBR"),
                SlotEnumOption::new("brg", "BRG"),
                SlotEnumOption::new("bgr", "BGR"),
            ],
        },
    }
}

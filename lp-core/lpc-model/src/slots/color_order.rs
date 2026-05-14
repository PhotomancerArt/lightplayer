use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
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

/// Revision-tracked RGB channel order.
pub type ColorOrderSlot = ValueSlot<ColorOrderValue>;

impl ToLpValue for ColorOrderValue {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for ColorOrderValue {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(&value)
                .ok_or_else(|| ValueRootError::new("expected RGB color order value")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ColorOrderValue {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("ColorOrderValue");

    fn value_shape() -> SlotValueShape {
        color_order_shape()
    }
}

pub fn color_order_shape() -> SlotValueShape {
    SlotValueShape {
        id: ColorOrderValue::SHAPE_ID,
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Dropdown {
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

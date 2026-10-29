use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError, ValueSlot,
};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// An ordered list of bus channel names, bare (`"palette"`, not
/// `"bus:palette"`). JSON: a plain string array. A module's declared
/// `inputs` (playlists-on-the-bus vision D4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, SlotValue)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ChannelNameList(pub Vec<String>);

impl From<Vec<String>> for ChannelNameList {
    fn from(value: Vec<String>) -> Self {
        Self(value)
    }
}

impl ChannelNameList {
    /// Whether `channel` is named.
    pub fn contains(&self, channel: &str) -> bool {
        self.0.iter().any(|name| name == channel)
    }
}

pub type ChannelNameListSlot = ValueSlot<ChannelNameList>;

impl ToLpValue for Vec<String> {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Array(self.iter().cloned().map(LpValue::String).collect())
    }
}

impl FromLpValue for Vec<String> {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Array(values) = value else {
            return Err(ValueRootError::new("expected a string list array"));
        };
        values
            .iter()
            .map(|value| match value {
                LpValue::String(name) => Ok(name.clone()),
                _ => Err(ValueRootError::new("expected a list of strings")),
            })
            .collect()
    }
}

impl SlotValue for Vec<String> {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("VecString");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::List(Box::new(LpType::String)),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn a_channel_name_list_is_a_plain_string_array() {
        let names = vec!["palette".to_string(), "detail".to_string()];
        assert_eq!(
            names.to_lp_value(),
            LpValue::Array(vec![
                LpValue::String("palette".to_string()),
                LpValue::String("detail".to_string())
            ])
        );
        assert_eq!(
            Vec::<String>::from_lp_value(&names.to_lp_value()).unwrap(),
            names
        );
        assert!(ChannelNameList(names).contains("detail"));
    }
}

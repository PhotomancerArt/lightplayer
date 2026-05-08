use super::{BindingDef, BindingDefError};
use crate::{
    FieldSlot, MapSlot, SlotDataAccess, SlotMapKeyShape, SlotMeta, SlotShape, SlotValueShape,
};
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::fmt;
use serde::{Deserialize, Serialize};

/// Authored bindings attached to a node definition.
///
/// The map key is the node-owned slot name. Each value declares whether that
/// slot consumes from a `source` or produces to a `target`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct BindingDefs(pub MapSlot<String, BindingDef>);

impl BindingDefs {
    pub fn new(entries: BTreeMap<String, BindingDef>) -> Self {
        Self(MapSlot::new(entries))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn entries(&self) -> &BTreeMap<String, BindingDef> {
        &self.0.entries
    }

    pub fn validate(&self) -> Result<(), BindingDefsError> {
        for (slot, binding) in &self.0.entries {
            binding
                .validate()
                .map_err(|source| BindingDefsError::InvalidBinding {
                    slot: slot.clone(),
                    source,
                })?;
        }
        Ok(())
    }
}

impl FieldSlot for BindingDefs {
    fn slot_field_shape() -> SlotShape {
        SlotShape::Map {
            meta: SlotMeta::empty(),
            key: SlotMapKeyShape::String,
            value: alloc::boxed::Box::new(SlotShape::Value {
                shape: SlotValueShape::raw(crate::LpType::Struct {
                    name: Some(String::from("BindingDef")),
                    fields: alloc::vec![
                        crate::ModelStructMember {
                            name: String::from("direction"),
                            ty: crate::LpType::String,
                        },
                        crate::ModelStructMember {
                            name: String::from("endpoint"),
                            ty: crate::LpType::String,
                        },
                    ],
                }),
            }),
        }
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Map(&self.0)
    }
}

/// Error returned by [`BindingDefs::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingDefsError {
    InvalidBinding {
        slot: String,
        source: BindingDefError,
    },
}

impl fmt::Display for BindingDefsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBinding { slot, source } => {
                write!(f, "invalid binding for slot {slot:?}: {source}")
            }
        }
    }
}

impl core::error::Error for BindingDefsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BindingEndpoint, SlotDataAccess};

    #[derive(Deserialize, Serialize)]
    struct Wrapper {
        bindings: BindingDefs,
    }

    #[test]
    fn toml_round_trips_binding_defs_as_nested_tables() {
        let toml = r#"
[bindings.output]
target = "bus#visual.out"
"#;
        let decoded: Wrapper = toml::from_str(toml).unwrap();
        assert!(matches!(
            decoded.bindings.entries()["output"].target,
            Some(BindingEndpoint::Bus(_))
        ));

        let encoded = toml::to_string(&decoded).unwrap();
        assert!(encoded.contains("[bindings.output]"));
        assert!(encoded.contains("target = \"bus#visual.out\""));
    }

    #[test]
    fn binding_defs_expose_slot_data_as_map() {
        let mut entries = BTreeMap::new();
        entries.insert(
            String::from("output"),
            BindingDef::target(BindingEndpoint::parse_ref("bus#visual.out").unwrap()),
        );
        let defs = BindingDefs::new(entries);

        assert!(matches!(defs.slot_field_data(), SlotDataAccess::Map(_)));
    }

    #[test]
    fn validate_reports_slot_name_for_invalid_binding() {
        let mut entries = BTreeMap::new();
        entries.insert(
            String::from("bad"),
            BindingDef {
                source: None,
                target: None,
            },
        );
        let defs = BindingDefs::new(entries);

        assert!(matches!(
            defs.validate(),
            Err(BindingDefsError::InvalidBinding { slot, .. }) if slot == "bad"
        ));
    }
}

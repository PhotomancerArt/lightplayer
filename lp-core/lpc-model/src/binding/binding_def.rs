use super::BindingEndpoint;
use crate::{
    FieldSlot, Revision, LpType, LpValue, ModelStructMember, SlotDataAccess, SlotShape,
    SlotValueAccess,
};
use alloc::string::{String, ToString};
use alloc::vec;
use core::fmt;
use serde::{Deserialize, Serialize};

/// Directional authored binding for one slot.
///
/// A binding is attached to a slot name by [`crate::BindingDefs`]. Consumed
/// slots use `source`; produced slots use `target`. Direction is validated
/// against the node's slot contract when the engine composes the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BindingDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<BindingEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<BindingEndpoint>,
}

impl BindingDef {
    pub fn source(source: BindingEndpoint) -> Self {
        Self {
            source: Some(source),
            target: None,
        }
    }

    pub fn target(target: BindingEndpoint) -> Self {
        Self {
            source: None,
            target: Some(target),
        }
    }

    pub fn validate(&self) -> Result<(), BindingDefError> {
        match (&self.source, &self.target) {
            (Some(_), Some(_)) => Err(BindingDefError::SourceAndTarget),
            (None, None) => Err(BindingDefError::MissingDirection),
            (_, Some(target)) if target.is_literal() => Err(BindingDefError::LiteralTarget),
            _ => Ok(()),
        }
    }

    fn direction_name(&self) -> &'static str {
        if self.source.is_some() {
            "source"
        } else if self.target.is_some() {
            "target"
        } else {
            "invalid"
        }
    }

    fn endpoint(&self) -> Option<&BindingEndpoint> {
        self.source.as_ref().or(self.target.as_ref())
    }
}

impl SlotValueAccess for BindingDef {
    fn changed_at(&self) -> Revision {
        crate::current_revision()
    }

    fn value(&self) -> LpValue {
        LpValue::Struct {
            name: Some(String::from("BindingDef")),
            fields: vec![
                (
                    String::from("direction"),
                    LpValue::String(String::from(self.direction_name())),
                ),
                (
                    String::from("endpoint"),
                    LpValue::String(
                        self.endpoint()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| String::from("<invalid>")),
                    ),
                ),
            ],
        }
    }
}

impl FieldSlot for BindingDef {
    fn slot_field_shape() -> SlotShape {
        SlotShape::value(LpType::Struct {
            name: Some(String::from("BindingDef")),
            fields: vec![
                ModelStructMember {
                    name: String::from("direction"),
                    ty: LpType::String,
                },
                ModelStructMember {
                    name: String::from("endpoint"),
                    ty: LpType::String,
                },
            ],
        })
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

/// Error returned by [`BindingDef::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingDefError {
    MissingDirection,
    SourceAndTarget,
    LiteralTarget,
}

impl fmt::Display for BindingDefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDirection => f.write_str("binding must specify source or target"),
            Self::SourceAndTarget => f.write_str("binding cannot specify both source and target"),
            Self::LiteralTarget => f.write_str("binding target cannot be a literal value"),
        }
    }
}

impl core::error::Error for BindingDefError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BindingEndpoint;

    #[test]
    fn validates_exactly_one_direction() {
        let source = BindingEndpoint::parse_ref("bus#visual.out").unwrap();
        let target = BindingEndpoint::parse_ref("bus#visual.out").unwrap();

        assert!(BindingDef::source(source.clone()).validate().is_ok());
        assert!(BindingDef::target(target.clone()).validate().is_ok());
        assert_eq!(
            BindingDef {
                source: Some(source),
                target: Some(target),
            }
            .validate(),
            Err(BindingDefError::SourceAndTarget)
        );
        assert_eq!(
            BindingDef {
                source: None,
                target: None,
            }
            .validate(),
            Err(BindingDefError::MissingDirection)
        );
    }

    #[test]
    fn rejects_literal_targets() {
        let binding = BindingDef::target(BindingEndpoint::Literal(LpValue::F32(1.0)));
        assert_eq!(binding.validate(), Err(BindingDefError::LiteralTarget));
    }
}

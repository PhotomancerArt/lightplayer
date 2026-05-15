use super::BindingEndpoint;
use crate::{OptionSlot, SlotRecord, ValueSlot};
use core::fmt;
use serde::{Deserialize, Serialize};

/// Directional authored binding for one slot.
///
/// A binding is attached to a slot name by [`crate::BindingDefs`]. Consumed
/// slots use `source`; produced slots use `target`. Direction is validated
/// against the node's slot contract when the engine composes the project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, SlotRecord)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BindingDef {
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub source: OptionSlot<ValueSlot<BindingEndpoint>>,
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub target: OptionSlot<ValueSlot<BindingEndpoint>>,
}

impl BindingDef {
    pub fn source(source: BindingEndpoint) -> Self {
        Self {
            source: OptionSlot::some(ValueSlot::new(source)),
            target: OptionSlot::none(),
        }
    }

    pub fn target(target: BindingEndpoint) -> Self {
        Self {
            source: OptionSlot::none(),
            target: OptionSlot::some(ValueSlot::new(target)),
        }
    }

    pub fn source_endpoint(&self) -> Option<&BindingEndpoint> {
        self.source.data.as_ref().map(ValueSlot::value)
    }

    pub fn target_endpoint(&self) -> Option<&BindingEndpoint> {
        self.target.data.as_ref().map(ValueSlot::value)
    }

    pub fn validate(&self) -> Result<(), BindingDefError> {
        let source = self
            .source_endpoint()
            .filter(|endpoint| !endpoint.is_unset());
        let target = self
            .target_endpoint()
            .filter(|endpoint| !endpoint.is_unset());

        match (source, target) {
            (Some(_), Some(_)) => Err(BindingDefError::SourceAndTarget),
            (None, None) => Err(BindingDefError::MissingDirection),
            (_, Some(target)) if target.is_literal() => Err(BindingDefError::LiteralTarget),
            _ => Ok(()),
        }
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
    use crate::{BindingEndpoint, LpValue};

    #[test]
    fn validates_exactly_one_direction() {
        let source = BindingEndpoint::parse_ref("bus#visual.out").unwrap();
        let target = BindingEndpoint::parse_ref("bus#visual.out").unwrap();

        assert!(BindingDef::source(source.clone()).validate().is_ok());
        assert!(BindingDef::target(target.clone()).validate().is_ok());
        assert_eq!(
            BindingDef {
                source: OptionSlot::some(ValueSlot::new(source)),
                target: OptionSlot::some(ValueSlot::new(target)),
            }
            .validate(),
            Err(BindingDefError::SourceAndTarget)
        );
        assert_eq!(
            BindingDef {
                source: OptionSlot::none(),
                target: OptionSlot::none(),
            }
            .validate(),
            Err(BindingDefError::MissingDirection)
        );
        assert_eq!(
            BindingDef::source(BindingEndpoint::Unset).validate(),
            Err(BindingDefError::MissingDirection)
        );
    }

    #[test]
    fn rejects_literal_targets() {
        let binding = BindingDef::target(BindingEndpoint::Literal(LpValue::F32(1.0)));
        assert_eq!(binding.validate(), Err(BindingDefError::LiteralTarget));
    }
}

use super::BindingRef;
use crate::{LpValue, OptionSlot, Slotted, ValueSlot};
use core::fmt;
use serde::{Deserialize, Serialize};

/// Authored binding attached to one local slot name.
///
/// The owning [`crate::BindingDefs`] map supplies the local slot. This record
/// supplies exactly one remote side:
///
/// - `value`: feed the local slot a literal value.
/// - `source`: feed the local slot from another slot or bus channel.
/// - `target`: publish the local slot to another slot or bus channel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BindingDef {
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub value: OptionSlot<ValueSlot<LpValue>>,
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub source: OptionSlot<ValueSlot<BindingRef>>,
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub target: OptionSlot<ValueSlot<BindingRef>>,
}

impl BindingDef {
    pub fn value(value: LpValue) -> Self {
        Self {
            value: OptionSlot::some(ValueSlot::new(value)),
            source: OptionSlot::default(),
            target: OptionSlot::default(),
        }
    }

    pub fn source(source: BindingRef) -> Self {
        Self {
            value: OptionSlot::default(),
            source: OptionSlot::some(ValueSlot::new(source)),
            target: OptionSlot::default(),
        }
    }

    pub fn target(target: BindingRef) -> Self {
        Self {
            value: OptionSlot::default(),
            source: OptionSlot::default(),
            target: OptionSlot::some(ValueSlot::new(target)),
        }
    }

    pub fn value_literal(&self) -> Option<&LpValue> {
        self.value.data.as_ref().map(ValueSlot::value)
    }

    pub fn source_ref(&self) -> Option<&BindingRef> {
        self.source.data.as_ref().map(ValueSlot::value)
    }

    pub fn target_ref(&self) -> Option<&BindingRef> {
        self.target.data.as_ref().map(ValueSlot::value)
    }

    pub fn validate(&self) -> Result<(), BindingDefError> {
        let count = usize::from(self.value_literal().is_some())
            + usize::from(self.source_ref().is_some())
            + usize::from(self.target_ref().is_some());

        match count {
            0 => Err(BindingDefError::MissingEndpoint),
            1 if self.source_ref().is_some_and(BindingRef::is_unset) => {
                Err(BindingDefError::UnsetRef)
            }
            1 if self.target_ref().is_some_and(BindingRef::is_unset) => {
                Err(BindingDefError::UnsetRef)
            }
            1 => Ok(()),
            _ => Err(BindingDefError::MultipleEndpoints),
        }
    }
}

/// Error returned by [`BindingDef::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingDefError {
    MissingEndpoint,
    MultipleEndpoints,
    UnsetRef,
}

impl fmt::Display for BindingDefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEndpoint => {
                f.write_str("binding must specify exactly one of value, source, or target")
            }
            Self::MultipleEndpoints => {
                f.write_str("binding cannot specify more than one of value, source, or target")
            }
            Self::UnsetRef => f.write_str("binding source or target cannot be unset"),
        }
    }
}

impl core::error::Error for BindingDefError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_exactly_one_endpoint() {
        let source = BindingRef::parse("bus#visual.out").unwrap();
        let target = BindingRef::parse("bus#visual.out").unwrap();

        assert!(BindingDef::source(source.clone()).validate().is_ok());
        assert!(BindingDef::target(target.clone()).validate().is_ok());
        assert!(BindingDef::value(LpValue::F32(1.0)).validate().is_ok());
        assert_eq!(
            BindingDef {
                value: OptionSlot::default(),
                source: OptionSlot::some(ValueSlot::new(source)),
                target: OptionSlot::some(ValueSlot::new(target)),
            }
            .validate(),
            Err(BindingDefError::MultipleEndpoints)
        );
        assert_eq!(
            BindingDef::default().validate(),
            Err(BindingDefError::MissingEndpoint)
        );
        assert_eq!(
            BindingDef::source(BindingRef::Unset).validate(),
            Err(BindingDefError::UnsetRef)
        );
    }

    #[test]
    fn serde_omits_unset_endpoint_slots() {
        let binding = BindingDef::target(BindingRef::parse("bus#visual.out").unwrap());

        let toml = toml::to_string(&binding).unwrap();

        assert!(!toml.contains("source"));
        assert!(!toml.contains("value"));
        assert!(toml.contains("target = \"bus#visual.out\""));
    }
}

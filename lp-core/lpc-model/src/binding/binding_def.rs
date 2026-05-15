use super::BindingEndpoint;
use crate::{
    OptionSlot, SlotCodec, SlotRecord, ValueSlot,
    slot_codec::{
        SlotObjectWriter, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError,
        SyntaxEventSource, ValueReader,
    },
};
use core::fmt;
use serde::{Deserialize, Serialize};

/// Directional authored binding for one slot.
///
/// A binding is attached to a slot name by [`crate::BindingDefs`]. Consumed
/// slots use `source`; produced slots use `target`. Direction is validated
/// against the node's slot contract when the engine composes the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SlotRecord)]
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
        match (self.source_endpoint(), self.target_endpoint()) {
            (Some(_), Some(_)) => Err(BindingDefError::SourceAndTarget),
            (None, None) => Err(BindingDefError::MissingDirection),
            (_, Some(target)) if target.is_literal() => Err(BindingDefError::LiteralTarget),
            _ => Ok(()),
        }
    }
}

impl SlotCodec for BindingDef {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        const FIELDS: &[&str] = &["source", "target"];
        let mut source = OptionSlot::none();
        let mut target = OptionSlot::none();
        let mut object = value.object()?;

        while let Some(mut prop) = object.next_prop()? {
            match prop.name() {
                "source" => {
                    source =
                        OptionSlot::some(ValueSlot::new(BindingEndpoint::read_slot(prop.value())?));
                }
                "target" => {
                    target =
                        OptionSlot::some(ValueSlot::new(BindingEndpoint::read_slot(prop.value())?));
                }
                other => return Err(prop.unknown_field(other, FIELDS)),
            }
        }

        Ok(Self { source, target })
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let mut object = value.object()?;
        write_endpoint_slot(&mut object, "source", &self.source)?;
        write_endpoint_slot(&mut object, "target", &self.target)?;
        object.finish()
    }
}

fn write_endpoint_slot<W>(
    object: &mut SlotObjectWriter<'_, W>,
    name: &str,
    endpoint: &OptionSlot<ValueSlot<BindingEndpoint>>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    if let Some(endpoint) = &endpoint.data {
        endpoint.value().write_slot(object.prop(name)?)?;
    }
    Ok(())
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
    }

    #[test]
    fn rejects_literal_targets() {
        let binding = BindingDef::target(BindingEndpoint::Literal(LpValue::F32(1.0)));
        assert_eq!(binding.validate(), Err(BindingDefError::LiteralTarget));
    }
}

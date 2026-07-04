use crate::{BindingDefs, ControlMessage, HwEndpointSpec, MapSlot, Slotted, ValueSlot};

pub const DEFAULT_BUTTON_ENDPOINT_SPEC: &str = "button:gpio:D9";

/// Authored hardware button input node definition.
///
/// The button is exposed as three stable-key control-message maps:
/// `down` for the press transition, `held` while the button remains pressed,
/// and `up` for the release transition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ButtonDef {
    /// Authored slot bindings for button outputs.
    pub bindings: BindingDefs,

    /// Hardware endpoint spec, for example `button:gpio:D9`.
    pub endpoint: ValueSlot<HwEndpointSpec>,

    /// Stable message id used as the key and payload id for this button.
    pub id: ValueSlot<u32>,

    /// Debounce duration in milliseconds.
    pub stable_ms: ValueSlot<u32>,
}

impl Default for ButtonDef {
    fn default() -> Self {
        Self {
            bindings: BindingDefs::default(),
            endpoint: default_endpoint(),
            id: default_id(),
            stable_ms: default_stable_ms(),
        }
    }
}

impl ButtonDef {
    pub const KIND: &'static str = "button";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Button
    }

    pub fn endpoint(&self) -> &HwEndpointSpec {
        self.endpoint.value()
    }
}

/// Runtime button state published to shader-compatible control maps.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct ButtonState {
    /// Present for one tick when the button transitions to pressed.
    #[slot(produced, map(key = "u32", value_ref = "lp::control::Message"))]
    pub down: MapSlot<u32, ControlMessage>,

    /// Present while the button is pressed.
    #[slot(produced, map(key = "u32", value_ref = "lp::control::Message"))]
    pub held: MapSlot<u32, ControlMessage>,

    /// Present for one tick when the button transitions to released.
    #[slot(produced, map(key = "u32", value_ref = "lp::control::Message"))]
    pub up: MapSlot<u32, ControlMessage>,
}

fn default_id() -> ValueSlot<u32> {
    ValueSlot::new(1)
}

fn default_endpoint() -> ValueSlot<HwEndpointSpec> {
    ValueSlot::new(HwEndpointSpec::from_static(DEFAULT_BUTTON_ENDPOINT_SPEC))
}

fn default_stable_ms() -> ValueSlot<u32> {
    ValueSlot::new(30)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeDef, SlotDirection, SlotShape, StaticSlotShape};

    #[test]
    fn button_def_parses_defaults() {
        let def = NodeDef::from_json_str(r#"{ "kind": "Button", "endpoint": "button:gpio:D9" }"#)
            .expect("button");

        let NodeDef::Button(def) = def else {
            panic!("button def");
        };
        assert_eq!(def.endpoint().as_str(), "button:gpio:D9");
        assert_eq!(*def.id.value(), 1);
        assert_eq!(*def.stable_ms.value(), 30);
    }

    #[test]
    fn button_state_maps_are_produced_control_messages() {
        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(ControlMessage::SHAPE_ID),
            Some(crate::CONTROL_MESSAGE_SHAPE_NAME)
        );

        let SlotShape::Record { fields, .. } = ButtonState::slot_shape() else {
            panic!("record shape");
        };

        for name in ["down", "held", "up"] {
            let field = fields
                .iter()
                .find(|field| field.name.as_str() == name)
                .expect("button state field");
            assert_eq!(field.semantics.direction, SlotDirection::Produced);
        }
    }
}

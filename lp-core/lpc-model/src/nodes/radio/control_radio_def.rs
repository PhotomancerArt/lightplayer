use crate::{BindingDefs, ControlMessage, HwEndpointSpec, MapSlot, Slotted, ValueSlot};

pub const DEFAULT_CONTROL_RADIO_ENDPOINT_SPEC: &str = "radio:espnow:0";
pub const DEFAULT_CONTROL_RADIO_CHANNEL: u32 = 1;
pub const DEFAULT_CONTROL_RADIO_REPEAT_COUNT: u32 = 3;
pub const DEFAULT_CONTROL_RADIO_WIFI_CHANNEL: u32 = 0;

/// Authored control-message radio bridge node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ControlRadioDef {
    /// Authored slot bindings for control radio input and output.
    pub bindings: BindingDefs,

    /// Hardware endpoint spec, for example `radio:espnow:0`.
    pub endpoint: ValueSlot<HwEndpointSpec>,

    /// Logical LightPlayer radio channel.
    pub channel: ValueSlot<u32>,

    /// Number of ticks to broadcast each newly accepted local message.
    pub repeat_count: ValueSlot<u32>,

    /// Physical Wi-Fi/ESP-NOW channel. Zero means driver default.
    pub wifi_channel: ValueSlot<u32>,

    /// Local control messages to broadcast.
    #[slot(
        consumed,
        merge = "by_key",
        map(key = "u32", value_ref = "lp::control::Message")
    )]
    pub input: MapSlot<u32, ControlMessage>,
}

impl Default for ControlRadioDef {
    fn default() -> Self {
        Self {
            bindings: BindingDefs::default(),
            endpoint: default_endpoint(),
            channel: default_channel(),
            repeat_count: default_repeat_count(),
            wifi_channel: default_wifi_channel(),
            input: MapSlot::default(),
        }
    }
}

impl ControlRadioDef {
    pub const KIND: &'static str = "ControlRadio";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::ControlRadio
    }

    pub fn endpoint(&self) -> &HwEndpointSpec {
        self.endpoint.value()
    }
}

/// Runtime control radio state.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct ControlRadioState {
    /// Accepted local and remote control messages for this tick.
    #[slot(produced, map(key = "u32", value_ref = "lp::control::Message"))]
    pub output: MapSlot<u32, ControlMessage>,
}

fn default_endpoint() -> ValueSlot<HwEndpointSpec> {
    ValueSlot::new(HwEndpointSpec::from_static(
        DEFAULT_CONTROL_RADIO_ENDPOINT_SPEC,
    ))
}

fn default_channel() -> ValueSlot<u32> {
    ValueSlot::new(DEFAULT_CONTROL_RADIO_CHANNEL)
}

fn default_repeat_count() -> ValueSlot<u32> {
    ValueSlot::new(DEFAULT_CONTROL_RADIO_REPEAT_COUNT)
}

fn default_wifi_channel() -> ValueSlot<u32> {
    ValueSlot::new(DEFAULT_CONTROL_RADIO_WIFI_CHANNEL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeDef, NodeKind, SlotDirection, SlotMerge, SlotShape, StaticSlotShape};

    #[test]
    fn control_radio_def_parses_defaults() {
        let def = NodeDef::from_json_str(r#"{ "kind": "ControlRadio" }"#).expect("control radio");

        let NodeDef::ControlRadio(def) = def else {
            panic!("control radio def");
        };
        assert_eq!(def.endpoint().as_str(), DEFAULT_CONTROL_RADIO_ENDPOINT_SPEC);
        assert_eq!(*def.channel.value(), DEFAULT_CONTROL_RADIO_CHANNEL);
        assert_eq!(
            *def.repeat_count.value(),
            DEFAULT_CONTROL_RADIO_REPEAT_COUNT
        );
        assert_eq!(
            *def.wifi_channel.value(),
            DEFAULT_CONTROL_RADIO_WIFI_CHANNEL
        );
        assert!(def.input.is_empty());
    }

    #[test]
    fn control_radio_input_is_consumed_and_merged_by_key() {
        let SlotShape::Record { fields, .. } = ControlRadioDef::slot_shape() else {
            panic!("record shape");
        };
        let input = fields
            .iter()
            .find(|field| field.name.as_str() == "input")
            .expect("input field");

        assert_eq!(input.semantics.direction, SlotDirection::Consumed);
        assert_eq!(input.semantics.merge, SlotMerge::ByKey);
    }

    #[test]
    fn control_radio_output_is_produced() {
        let SlotShape::Record { fields, .. } = ControlRadioState::slot_shape() else {
            panic!("record shape");
        };
        let output = fields
            .iter()
            .find(|field| field.name.as_str() == "output")
            .expect("output field");

        assert_eq!(output.semantics.direction, SlotDirection::Produced);
    }

    #[test]
    fn node_def_delegates_control_radio_kind() {
        let def = NodeDef::ControlRadio(ControlRadioDef::default());

        assert_eq!(def.kind(), NodeKind::ControlRadio);
        assert_eq!(def.kind_name(), "ControlRadio");
        assert_eq!(def.variant_name(), "ControlRadio");
        assert!(def.as_control_radio().is_some());
    }
}

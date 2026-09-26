use crate::{
    BindingDefs, ControlMessage, HwEndpointSpec, MapSlot, PowerButtonMode, Slotted, ValueSlot,
};

/// Default endpoint: `D0`, the first XIAO ESP32-C6 pin that can wake the chip
/// from deep sleep (only LP GPIO 0–7 can).
pub const DEFAULT_POWER_BUTTON_ENDPOINT_SPEC: &str = "button:local:D0";

/// Authored power-off control: a button or switch that puts the device into
/// deep sleep, and wakes it again.
///
/// Waking is a reset, so the project simply loads again. The endpoint must be
/// a pin the board manifest marks `deep-sleep-wake`; a pin that cannot wake
/// the chip is refused before anything sleeps.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct PowerButtonDef {
    /// Authored slot bindings for the short-click output.
    pub bindings: BindingDefs,

    /// Hardware endpoint spec, for example `button:local:D0`.
    pub endpoint: ValueSlot<HwEndpointSpec>,

    /// Momentary button (`hold`) or latching on/off switch (`switch`).
    pub mode: ValueSlot<PowerButtonMode>,

    /// Stable message id used as the key and payload id for short clicks.
    pub id: ValueSlot<u32>,

    /// Debounce duration in milliseconds.
    pub stable_ms: ValueSlot<u32>,

    /// `hold` mode: how long the button must be held before powering off.
    pub hold_ms: ValueSlot<u32>,
}

impl Default for PowerButtonDef {
    fn default() -> Self {
        Self {
            bindings: BindingDefs::default(),
            endpoint: ValueSlot::new(HwEndpointSpec::from_static(
                DEFAULT_POWER_BUTTON_ENDPOINT_SPEC,
            )),
            mode: ValueSlot::new(PowerButtonMode::default()),
            id: ValueSlot::new(1),
            stable_ms: ValueSlot::new(30),
            hold_ms: ValueSlot::new(1500),
        }
    }
}

impl PowerButtonDef {
    pub const KIND: &'static str = "power_button";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::PowerButton
    }

    pub fn endpoint(&self) -> &HwEndpointSpec {
        self.endpoint.value()
    }
}

/// Runtime power-button state published to shader-compatible control maps.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
#[slot(default_role = "state")]
pub struct PowerButtonState {
    /// `hold` mode: present for one tick when a press is released before the
    /// hold threshold. Never produced in `switch` mode.
    #[slot(produced, map(key = "u32", value_ref = "lp::control::Message"))]
    pub click: MapSlot<u32, ControlMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeDef;

    #[test]
    fn power_button_def_parses_defaults() {
        let def = NodeDef::from_json_str(r#"{ "kind": "PowerButton" }"#).expect("power button");

        let NodeDef::PowerButton(def) = def else {
            panic!("power button def");
        };
        assert_eq!(def.endpoint().as_str(), DEFAULT_POWER_BUTTON_ENDPOINT_SPEC);
        assert_eq!(*def.mode.value(), PowerButtonMode::Hold);
        assert_eq!(*def.hold_ms.value(), 1500);
        assert_eq!(*def.stable_ms.value(), 30);
    }

    #[test]
    fn power_button_def_parses_switch_mode() {
        let def = NodeDef::from_json_str(
            r#"{ "kind": "PowerButton", "endpoint": "button:local:D1", "mode": "switch" }"#,
        )
        .expect("power button");

        let NodeDef::PowerButton(def) = def else {
            panic!("power button def");
        };
        assert_eq!(def.endpoint().as_str(), "button:local:D1");
        assert_eq!(*def.mode.value(), PowerButtonMode::Switch);
    }
}

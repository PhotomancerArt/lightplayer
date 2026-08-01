//! Small typed form controls shared by every section. Each control renders a
//! label + input pair and reports parsed values through an `EventHandler` —
//! parsing lives here so section code stays declarative.

use dioxus::prelude::*;
use lpa_boards::{CapKind, PinRole, SupportTier, UsbBridge};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TextField(
    label: &'static str,
    value: String,
    on_change: EventHandler<String>,
    #[props(default)] placeholder: Option<&'static str>,
    /// Render the input monospace (ids, labels, technical strings).
    #[props(default = false)]
    mono: bool,
) -> Element {
    rsx! {
        div { class: "lpb-ed-field",
            label { "{label}" }
            input {
                r#type: "text",
                class: if mono { "lpb-ed-input lpb-ed-input--mono" } else { "lpb-ed-input" },
                value: "{value}",
                placeholder: placeholder.unwrap_or(""),
                oninput: move |event| on_change.call(event.value()),
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TextAreaField(
    label: &'static str,
    value: String,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "lpb-ed-field lpb-ed-field--wide",
            label { "{label}" }
            textarea {
                class: "lpb-ed-input lpb-ed-textarea",
                value: "{value}",
                oninput: move |event| on_change.call(event.value()),
            }
        }
    }
}

/// Optional text: an empty input reads as `None`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptTextField(
    label: &'static str,
    value: Option<String>,
    on_change: EventHandler<Option<String>>,
    #[props(default)] placeholder: Option<&'static str>,
) -> Element {
    let shown = value.unwrap_or_default();
    rsx! {
        div { class: "lpb-ed-field",
            label { "{label}" }
            input {
                r#type: "text",
                class: "lpb-ed-input",
                value: "{shown}",
                placeholder: placeholder.unwrap_or("—"),
                oninput: move |event| {
                    let text = event.value();
                    on_change.call((!text.trim().is_empty()).then_some(text));
                },
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NumField(
    label: &'static str,
    value: f64,
    on_change: EventHandler<f64>,
    #[props(default = 1.0)] step: f64,
) -> Element {
    let shown = if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    };
    rsx! {
        div { class: "lpb-ed-field lpb-ed-field--num",
            label { "{label}" }
            input {
                r#type: "number",
                class: "lpb-ed-input",
                step: "{step}",
                value: "{shown}",
                oninput: move |event| {
                    if let Ok(parsed) = event.value().parse::<f64>()
                        && parsed.is_finite()
                    {
                        on_change.call(parsed);
                    }
                },
            }
        }
    }
}

/// Optional GPIO number: empty reads as `None`, non-numbers are ignored.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptGpioField(
    value: Option<u8>,
    on_change: EventHandler<Option<u8>>,
    #[props(default)] label: Option<&'static str>,
) -> Element {
    let shown = value.map(|gpio| gpio.to_string()).unwrap_or_default();
    rsx! {
        div { class: "lpb-ed-field lpb-ed-field--num",
            if let Some(label) = label {
                label { "{label}" }
            }
            input {
                r#type: "text",
                class: "lpb-ed-input lpb-ed-input--mono lpb-ed-input--gpio",
                inputmode: "numeric",
                placeholder: "—",
                value: "{shown}",
                oninput: move |event| {
                    let text = event.value();
                    if text.trim().is_empty() {
                        on_change.call(None);
                    } else if let Ok(parsed) = text.trim().parse::<u8>() {
                        on_change.call(Some(parsed));
                    }
                },
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CheckField(
    label: &'static str,
    value: bool,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "lpb-ed-check",
            input {
                r#type: "checkbox",
                checked: value,
                onchange: move |event| on_change.call(event.checked()),
            }
            "{label}"
        }
    }
}

// ---- enum selects --------------------------------------------------------
//
// Explicit (variant, value) tables rather than serde round-trips: the strings
// are UI vocabulary, and a select must never silently drop a variant.

pub const ROLES: &[(PinRole, &str)] = &[
    (PinRole::Io, "io"),
    (PinRole::IoIn, "io-in"),
    (PinRole::Strap, "strap"),
    (PinRole::Pwr5, "pwr5"),
    (PinRole::Pwr3, "pwr3"),
    (PinRole::Gnd, "gnd"),
    (PinRole::Usb, "usb"),
    (PinRole::Ctl, "ctl"),
    (PinRole::Nc, "nc"),
    (PinRole::Rsvd, "rsvd"),
];

pub const CAP_KINDS: &[(CapKind, &str)] = &[
    (CapKind::Adc, "adc"),
    (CapKind::Dac, "dac"),
    (CapKind::Touch, "touch"),
    (CapKind::Spi, "spi"),
    (CapKind::I2c, "i2c"),
    (CapKind::Uart, "uart"),
    (CapKind::Usb, "usb"),
    (CapKind::Strap, "strap"),
    (CapKind::Pwr, "pwr"),
    (CapKind::Warn, "warn"),
    (CapKind::Note, "note"),
];

pub const TIERS: &[(SupportTier, &str)] = &[
    (SupportTier::Gold, "gold"),
    (SupportTier::Silver, "silver"),
    (SupportTier::Bronze, "bronze"),
];

pub const BRIDGES: &[(UsbBridge, &str)] = &[
    (UsbBridge::NativeUsbJtag, "native-usb-jtag"),
    (UsbBridge::Ch340G, "ch340g"),
    (UsbBridge::Ch340C, "ch340c"),
    (UsbBridge::Ch340K, "ch340k"),
    (UsbBridge::Ch9102F, "ch9102f"),
    (UsbBridge::Cp2102, "cp2102"),
];

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RoleSelect(value: PinRole, on_change: EventHandler<PinRole>) -> Element {
    rsx! {
        select {
            class: "lpb-ed-select",
            onchange: move |event| {
                if let Some((role, _)) = ROLES.iter().find(|(_, name)| *name == event.value()) {
                    on_change.call(*role);
                }
            },
            for (role, name) in ROLES {
                option { value: "{name}", selected: *role == value, "{name}" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CapKindSelect(value: CapKind, on_change: EventHandler<CapKind>) -> Element {
    rsx! {
        select {
            class: "lpb-ed-select",
            onchange: move |event| {
                if let Some((kind, _)) = CAP_KINDS.iter().find(|(_, name)| *name == event.value())
                {
                    on_change.call(*kind);
                }
            },
            for (kind, name) in CAP_KINDS {
                option { value: "{name}", selected: *kind == value, "{name}" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TierSelect(value: SupportTier, on_change: EventHandler<SupportTier>) -> Element {
    rsx! {
        div { class: "lpb-ed-field",
            label { "tier" }
            select {
                class: "lpb-ed-select",
                onchange: move |event| {
                    if let Some((tier, _)) = TIERS.iter().find(|(_, name)| *name == event.value())
                    {
                        on_change.call(*tier);
                    }
                },
                for (tier, name) in TIERS {
                    option { value: "{name}", selected: *tier == value, "{name}" }
                }
            }
        }
    }
}

/// USB bridge select with an explicit "unset" — the authoring policy is to
/// leave it out until the chip is verified.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BridgeSelect(
    value: Option<UsbBridge>,
    on_change: EventHandler<Option<UsbBridge>>,
) -> Element {
    rsx! {
        div { class: "lpb-ed-field",
            label { "usb_bridge" }
            select {
                class: "lpb-ed-select",
                onchange: move |event| {
                    let picked = BRIDGES
                        .iter()
                        .find(|(_, name)| *name == event.value())
                        .map(|(bridge, _)| *bridge);
                    on_change.call(picked);
                },
                option { value: "", selected: value.is_none(), "unset (unverified)" }
                for (bridge, name) in BRIDGES {
                    option { value: "{name}", selected: value == Some(*bridge), "{name}" }
                }
            }
        }
    }
}

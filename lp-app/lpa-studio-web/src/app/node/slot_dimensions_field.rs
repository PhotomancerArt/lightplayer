//! Compact width × height field for `Dimensions`-hinted struct slot values.
//!
//! `DimensionsSlotField` renders a `Dim2u`-shaped struct value (`width` and
//! `height` u32 fields) as one paired control instead of a generic struct
//! display. When the slot is editable and addressed, both components render
//! as number inputs (`onchange` semantics — dispatch on blur/enter, per
//! roadmap D5). Editing one component read-modify-writes the WHOLE struct
//! `LpValue` and dispatches a single `SetValue` (plan D3 — one address per
//! leaf).

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotValueKind};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::{field_wiring, numeric_field_class, parse_u32_input};

/// The width/height component pair carried by a dimensions struct value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DimensionsParts {
    pub width: u32,
    pub height: u32,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DimensionsSlotField(
    kind: UiSlotValueKind,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some(parts) = dimensions_parts(&kind) else {
        return rsx! {};
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();

    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: numeric_field_class(&state), title: "{invalid_title}",
                span { class: "tw:font-mono", "{parts.width}" }
                span { class: separator_class(), "\u{d7}" }
                span { class: "tw:font-mono", "{parts.height}" }
            }
        };
    };

    rsx! {
        span { class: numeric_field_class(&state), title: "{invalid_title}",
            DimensionComponentInput {
                kind: kind.clone(),
                component: DimensionComponent::Width,
                value: parts.width,
                address: address.clone(),
                handler,
            }
            span { class: separator_class(), "\u{d7}" }
            DimensionComponentInput {
                kind,
                component: DimensionComponent::Height,
                value: parts.height,
                address,
                handler,
            }
        }
    }
}

/// Which half of the width × height pair an input edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DimensionComponent {
    Width,
    Height,
}

impl DimensionComponent {
    fn field_name(self) -> &'static str {
        match self {
            Self::Width => "width",
            Self::Height => "height",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Width => "Width",
            Self::Height => "Height",
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DimensionComponentInput(
    kind: UiSlotValueKind,
    component: DimensionComponent,
    value: u32,
    address: ProjectSlotAddress,
    handler: EventHandler<UiAction>,
) -> Element {
    rsx! {
        input {
            class: "tw:w-12 tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-right tw:font-mono tw:text-inherit tw:outline-none",
            r#type: "number",
            min: "0",
            step: "1",
            value: "{value}",
            aria_label: component.label(),
            title: component.label(),
            onchange: move |event| {
                if let Some(next) = dimensions_set_component(&kind, component, &event.value()) {
                    handler.call(slot_set_value_action(address.clone(), next));
                }
            },
        }
    }
}

fn separator_class() -> &'static str {
    "tw:flex-none tw:text-subtle-foreground"
}

/// Extract the width/height pair from a dimensions struct value. `None` when
/// the kind is not a `Dim2u`-shaped struct (exactly two u32 fields named
/// `width` and `height`) — the caller falls back to the generic display.
pub(crate) fn dimensions_parts(kind: &UiSlotValueKind) -> Option<DimensionsParts> {
    let UiSlotValueKind::Struct { fields, .. } = kind else {
        return None;
    };
    if fields.len() != 2 {
        return None;
    }
    let component = |name: &str| {
        fields
            .iter()
            .find_map(|(field, value)| match (&value.kind, field == name) {
                (UiSlotValueKind::U32(value), true) => Some(*value),
                _ => None,
            })
    };
    Some(DimensionsParts {
        width: component("width")?,
        height: component("height")?,
    })
}

/// Replace one component (parsed as u32 from `raw`) and return the composed
/// WHOLE struct value for a single `SetValue` dispatch, preserving the struct
/// name and field order. `None` means "do not dispatch" (not a dimensions
/// struct or no parse).
pub(crate) fn dimensions_set_component(
    kind: &UiSlotValueKind,
    component: DimensionComponent,
    raw: &str,
) -> Option<LpValue> {
    let parts = dimensions_parts(kind)?;
    let next = parse_u32_input(raw)?;
    let (width, height) = match component {
        DimensionComponent::Width => (next, parts.height),
        DimensionComponent::Height => (parts.width, next),
    };
    let UiSlotValueKind::Struct { name, fields } = kind else {
        return None;
    };
    Some(LpValue::Struct {
        name: name.clone(),
        fields: fields
            .iter()
            .map(|(field, _)| {
                let value = if field == DimensionComponent::Width.field_name() {
                    width
                } else {
                    height
                };
                (field.clone(), LpValue::U32(value))
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{DimensionComponent, DimensionsParts, dimensions_parts, dimensions_set_component};
    use lpa_studio_core::{LpValue, UiSlotValue, UiSlotValueKind};

    fn dim2u_kind(width: u32, height: u32) -> UiSlotValueKind {
        UiSlotValueKind::Struct {
            name: Some("Dim2u".to_string()),
            fields: vec![
                ("width".to_string(), UiSlotValue::u32(width)),
                ("height".to_string(), UiSlotValue::u32(height)),
            ],
        }
    }

    #[test]
    fn extracts_width_and_height_from_dim2u_struct() {
        assert_eq!(
            dimensions_parts(&dim2u_kind(32, 18)),
            Some(DimensionsParts {
                width: 32,
                height: 18,
            })
        );
    }

    #[test]
    fn rejects_non_dimensions_kinds() {
        assert_eq!(dimensions_parts(&UiSlotValueKind::UVec2([32, 18])), None);
        assert_eq!(
            dimensions_parts(&UiSlotValueKind::Struct {
                name: None,
                fields: vec![("width".to_string(), UiSlotValue::u32(32))],
            }),
            None
        );
        assert_eq!(
            dimensions_parts(&UiSlotValueKind::Struct {
                name: None,
                fields: vec![
                    ("width".to_string(), UiSlotValue::f32(1.0)),
                    ("height".to_string(), UiSlotValue::u32(18)),
                ],
            }),
            None
        );
    }

    #[test]
    fn composes_whole_struct_preserving_name_and_order() {
        let value = dimensions_set_component(&dim2u_kind(32, 18), DimensionComponent::Height, "24");

        assert_eq!(
            value,
            Some(LpValue::Struct {
                name: Some("Dim2u".to_string()),
                fields: vec![
                    ("width".to_string(), LpValue::U32(32)),
                    ("height".to_string(), LpValue::U32(24)),
                ],
            })
        );
    }

    #[test]
    fn clamps_negative_input_to_zero_and_rejects_garbage() {
        let clamped =
            dimensions_set_component(&dim2u_kind(32, 18), DimensionComponent::Width, "-4");
        assert_eq!(
            clamped,
            Some(LpValue::Struct {
                name: Some("Dim2u".to_string()),
                fields: vec![
                    ("width".to_string(), LpValue::U32(0)),
                    ("height".to_string(), LpValue::U32(18)),
                ],
            })
        );
        assert_eq!(
            dimensions_set_component(&dim2u_kind(32, 18), DimensionComponent::Width, "abc"),
            None
        );
    }
}

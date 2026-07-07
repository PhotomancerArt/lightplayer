//! Labeled six-parameter grid for `Affine2d`-hinted `Mat3x3` slot values.
//!
//! `Affine2dSlotField` renders the six active parameters of a 2D affine
//! transform — `a b tx / c d ty` mirroring the wire layout
//! `[[a, b, tx], [c, d, ty], [0, 0, 1]]` (`lpc-model` `Affine2d`) — instead
//! of a raw 3×3 matrix grid. When the slot is editable and addressed, cells
//! render as number inputs (`onchange` semantics — dispatch on blur/enter,
//! per roadmap D5). Editing one parameter read-modify-writes the WHOLE
//! `Mat3x3` `LpValue` with the inactive bottom row FIXED to `[0, 0, 1]` and
//! dispatches a single `SetValue` (plan D3 — one address per leaf).

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotValueKind};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::{field_wiring, format_float, parse_f32_input};

/// The six active affine parameters in display order (`a b tx / c d ty`),
/// each naming its `Mat3x3` cell.
pub(crate) const AFFINE2D_PARAMS: [Affine2dParam; 6] = [
    Affine2dParam::new("a", 0, 0),
    Affine2dParam::new("b", 0, 1),
    Affine2dParam::new("tx", 0, 2),
    Affine2dParam::new("c", 1, 0),
    Affine2dParam::new("d", 1, 1),
    Affine2dParam::new("ty", 1, 2),
];

/// One labeled affine parameter and the `Mat3x3` cell it edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Affine2dParam {
    pub label: &'static str,
    pub row: usize,
    pub col: usize,
}

impl Affine2dParam {
    const fn new(label: &'static str, row: usize, col: usize) -> Self {
        Self { label, row, col }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn Affine2dSlotField(
    kind: UiSlotValueKind,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let UiSlotValueKind::Mat3x3(_) = &kind else {
        return rsx! {};
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();
    let frame_class = affine_frame_class(&state);

    rsx! {
        span { class: frame_class, title: "{invalid_title}",
            for param in AFFINE2D_PARAMS {
                Affine2dParamCell {
                    kind: kind.clone(),
                    param,
                    state: state.clone(),
                    address: address.clone(),
                    on_action,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn Affine2dParamCell(
    kind: UiSlotValueKind,
    param: Affine2dParam,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let display = affine_param_display(&kind, param);

    rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1",
            small { class: "tw:text-[0.64rem] tw:font-bold tw:lowercase tw:text-subtle-foreground",
                "{param.label}"
            }
            if let Some((address, handler)) = field_wiring(&state, &address, on_action) {
                input {
                    class: "tw:w-10 tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-right tw:font-mono tw:text-xs tw:text-inherit tw:outline-none",
                    r#type: "number",
                    step: "any",
                    value: "{display}",
                    aria_label: "{param.label}",
                    onchange: move |event| {
                        if let Some(next) = affine2d_set_param(&kind, param, &event.value()) {
                            handler.call(slot_set_value_action(address.clone(), next));
                        }
                    },
                }
            } else {
                span { class: "tw:text-right tw:font-mono tw:text-xs", "{display}" }
            }
        }
    }
}

fn affine_param_display(kind: &UiSlotValueKind, param: Affine2dParam) -> String {
    let UiSlotValueKind::Mat3x3(rows) = kind else {
        return String::new();
    };
    format_float(rows[param.row][param.col])
}

/// Replace one affine parameter (parsed from `raw`) and return the composed
/// WHOLE `Mat3x3` value for a single `SetValue` dispatch, with the inactive
/// bottom row fixed to `[0, 0, 1]` (the `Affine2d` wire contract). `None`
/// means "do not dispatch" (non-`Mat3x3` kind or no parse).
pub(crate) fn affine2d_set_param(
    kind: &UiSlotValueKind,
    param: Affine2dParam,
    raw: &str,
) -> Option<LpValue> {
    let UiSlotValueKind::Mat3x3(rows) = kind else {
        return None;
    };
    if param.row >= 2 || param.col >= 3 {
        return None;
    }
    let value = parse_f32_input(raw)?;
    let mut next = *rows;
    next[param.row][param.col] = value;
    next[2] = [0.0, 0.0, 1.0];
    Some(LpValue::Mat3x3(next))
}

fn affine_frame_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-grid tw:min-w-0 tw:grid-cols-3 tw:gap-x-2 tw:gap-y-0.5 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-grid tw:min-w-0 tw:grid-cols-3 tw:gap-x-2 tw:gap-y-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-muted-foreground"
    } else {
        "tw:inline-grid tw:min-w-0 tw:grid-cols-3 tw:gap-x-2 tw:gap-y-0.5 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-subtle-foreground"
    }
}

#[cfg(test)]
mod tests {
    use super::{AFFINE2D_PARAMS, affine2d_set_param};
    use lpa_studio_core::{LpValue, UiSlotValueKind};

    fn identity() -> UiSlotValueKind {
        UiSlotValueKind::Mat3x3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
    }

    #[test]
    fn params_cover_the_two_active_rows_in_wire_order() {
        let cells: Vec<(usize, usize)> = AFFINE2D_PARAMS
            .iter()
            .map(|param| (param.row, param.col))
            .collect();

        assert_eq!(cells, [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
        assert_eq!(
            AFFINE2D_PARAMS.map(|param| param.label),
            ["a", "b", "tx", "c", "d", "ty"]
        );
    }

    #[test]
    fn composes_whole_matrix_from_one_param() {
        let tx = AFFINE2D_PARAMS[2];

        let value = affine2d_set_param(&identity(), tx, "12");

        assert_eq!(
            value,
            Some(LpValue::Mat3x3([
                [1.0, 0.0, 12.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ]))
        );
    }

    #[test]
    fn fixes_a_drifted_inactive_row_on_write() {
        let drifted = UiSlotValueKind::Mat3x3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5, 0.5]]);
        let a = AFFINE2D_PARAMS[0];

        let value = affine2d_set_param(&drifted, a, "2");

        assert_eq!(
            value,
            Some(LpValue::Mat3x3([
                [2.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ]))
        );
    }

    #[test]
    fn rejects_unparseable_input_and_non_matrix_kinds() {
        let a = AFFINE2D_PARAMS[0];

        assert_eq!(affine2d_set_param(&identity(), a, "abc"), None);
        assert_eq!(affine2d_set_param(&identity(), a, "inf"), None);
        assert_eq!(affine2d_set_param(&UiSlotValueKind::F32(1.0), a, "2"), None);
    }
}

//! Compact per-cell grid field for matrix slot values.
//!
//! `MatrixSlotField` covers `Mat2x2`/`Mat3x3`/`Mat4x4`. These are rare and
//! read-mostly, so the grid stays visually compact: small mono cells that
//! become number inputs when the slot is editable and addressed (`onchange`
//! semantics — dispatch on blur/enter, per roadmap D5). Editing one cell
//! read-modify-writes the WHOLE matrix `LpValue` and dispatches a single
//! `SetValue` (plan D3 — one address per leaf).

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotValueKind};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::{field_wiring, format_float, parse_f32_input};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MatrixSlotField(
    kind: UiSlotValueKind,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some((rows, cols)) = matrix_dims(&kind) else {
        return rsx! {};
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();
    let frame_class = matrix_frame_class(&state);
    let cols_class = matrix_cols_class(cols);

    rsx! {
        span { class: "{frame_class} {cols_class}", title: "{invalid_title}",
            for row in 0..rows {
                for col in 0..cols {
                    MatrixCellField {
                        kind: kind.clone(),
                        row,
                        col,
                        state: state.clone(),
                        address: address.clone(),
                        on_action,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn MatrixCellField(
    kind: UiSlotValueKind,
    row: usize,
    col: usize,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let display = matrix_cell_display(&kind, row, col);

    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: "tw:text-right tw:font-mono tw:text-xs", "{display}" }
        };
    };

    rsx! {
        input {
            class: "tw:w-10 tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-right tw:font-mono tw:text-xs tw:text-inherit tw:outline-none",
            r#type: "number",
            step: "any",
            value: "{display}",
            onchange: move |event| {
                if let Some(next) = matrix_set_cell(&kind, row, col, &event.value()) {
                    handler.call(slot_set_value_action(address.clone(), next));
                }
            },
        }
    }
}

fn matrix_dims(kind: &UiSlotValueKind) -> Option<(usize, usize)> {
    match kind {
        UiSlotValueKind::Mat2x2(_) => Some((2, 2)),
        UiSlotValueKind::Mat3x3(_) => Some((3, 3)),
        UiSlotValueKind::Mat4x4(_) => Some((4, 4)),
        _ => None,
    }
}

fn matrix_cell_display(kind: &UiSlotValueKind, row: usize, col: usize) -> String {
    let cell = match kind {
        UiSlotValueKind::Mat2x2(rows) => rows.get(row).and_then(|cells| cells.get(col)),
        UiSlotValueKind::Mat3x3(rows) => rows.get(row).and_then(|cells| cells.get(col)),
        UiSlotValueKind::Mat4x4(rows) => rows.get(row).and_then(|cells| cells.get(col)),
        _ => None,
    };
    cell.copied().map(format_float).unwrap_or_default()
}

/// Replace one cell and return the composed WHOLE matrix value for a single
/// `SetValue` dispatch. `None` means "do not dispatch" (non-matrix kind, bad
/// cell coordinates, or no parse).
pub(crate) fn matrix_set_cell(
    kind: &UiSlotValueKind,
    row: usize,
    col: usize,
    raw: &str,
) -> Option<LpValue> {
    let value = parse_f32_input(raw)?;
    Some(match kind {
        UiSlotValueKind::Mat2x2(rows) => LpValue::Mat2x2(with_cell(rows, row, col, value)?),
        UiSlotValueKind::Mat3x3(rows) => LpValue::Mat3x3(with_cell(rows, row, col, value)?),
        UiSlotValueKind::Mat4x4(rows) => LpValue::Mat4x4(with_cell(rows, row, col, value)?),
        _ => return None,
    })
}

fn with_cell<const R: usize, const C: usize>(
    rows: &[[f32; C]; R],
    row: usize,
    col: usize,
    value: f32,
) -> Option<[[f32; C]; R]> {
    if row >= R || col >= C {
        return None;
    }
    let mut next = *rows;
    next[row][col] = value;
    Some(next)
}

fn matrix_frame_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-grid tw:min-w-0 tw:gap-x-2 tw:gap-y-0.5 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-grid tw:min-w-0 tw:gap-x-2 tw:gap-y-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-muted-foreground"
    } else {
        "tw:inline-grid tw:min-w-0 tw:gap-x-2 tw:gap-y-0.5 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-subtle-foreground"
    }
}

fn matrix_cols_class(cols: usize) -> &'static str {
    match cols {
        2 => "tw:grid-cols-2",
        3 => "tw:grid-cols-3",
        _ => "tw:grid-cols-4",
    }
}

#[cfg(test)]
mod tests {
    use super::matrix_set_cell;
    use lpa_studio_core::{LpValue, UiSlotValueKind};

    #[test]
    fn composes_whole_matrix_from_one_cell() {
        let kind = UiSlotValueKind::Mat2x2([[1.0, 0.0], [0.0, 1.0]]);

        let value = matrix_set_cell(&kind, 1, 0, "0.5");

        assert_eq!(value, Some(LpValue::Mat2x2([[1.0, 0.0], [0.5, 1.0]])));
    }

    #[test]
    fn rejects_bad_cells_and_non_matrix_kinds() {
        let kind = UiSlotValueKind::Mat3x3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

        assert_eq!(matrix_set_cell(&kind, 3, 0, "1.0"), None);
        assert_eq!(matrix_set_cell(&kind, 0, 3, "1.0"), None);
        assert_eq!(matrix_set_cell(&kind, 0, 0, "abc"), None);
        assert_eq!(
            matrix_set_cell(&UiSlotValueKind::F32(1.0), 0, 0, "1.0"),
            None
        );
    }
}

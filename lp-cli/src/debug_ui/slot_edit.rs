//! Small editable slot-leaf helpers for the temporary debug UI.

use std::collections::BTreeMap;

use eframe::egui;
use lpc_model::{LpValue, SlotPath, SlotPolicy, SlotValueShape, ValueEditorHint};

/// Stable UI key for a slot mutation target.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SlotEditKey {
    pub root: String,
    pub path: SlotPath,
}

impl SlotEditKey {
    pub fn new(root: impl Into<String>, path: SlotPath) -> Self {
        Self {
            root: root.into(),
            path,
        }
    }
}

/// User edit produced by rendering one writable slot value leaf.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SlotEditIntent {
    pub root: String,
    pub path: SlotPath,
    pub value: LpValue,
}

impl SlotEditIntent {
    pub fn key(&self) -> SlotEditKey {
        SlotEditKey::new(self.root.clone(), self.path.clone())
    }
}

/// Read-only edit status lookup for value rows.
pub(crate) struct SlotEditStatusContext<'a> {
    errors_by_slot: &'a BTreeMap<SlotEditKey, String>,
}

impl<'a> SlotEditStatusContext<'a> {
    pub fn new(errors_by_slot: &'a BTreeMap<SlotEditKey, String>) -> Self {
        Self { errors_by_slot }
    }

    pub fn status(&self, root: &str, path: &SlotPath) -> SlotEditStatus<'_> {
        let key = SlotEditKey::new(root, path.clone());
        SlotEditStatus {
            error: self.errors_by_slot.get(&key).map(String::as_str),
        }
    }
}

/// Per-row edit status.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SlotEditStatus<'a> {
    pub error: Option<&'a str>,
}

/// Render a supported editor for one slot value leaf.
///
/// Returns `Some(value)` only when the user changed a supported writable value.
pub(crate) fn render_slot_value_editor(
    ui: &mut egui::Ui,
    root: &str,
    path: &SlotPath,
    shape: &SlotValueShape,
    policy: SlotPolicy,
    value: &LpValue,
) -> Option<LpValue> {
    if !policy.writable {
        return None;
    }

    match value {
        LpValue::Bool(value) => render_bool_editor(ui, *value),
        LpValue::F32(value) => render_f32_editor(ui, shape, *value),
        LpValue::String(value) => render_string_editor(ui, root, path, shape, value),
        _ => None,
    }
}

pub(crate) fn slot_value_editor_supported(shape: &SlotValueShape, value: &LpValue) -> bool {
    match value {
        LpValue::Bool(_) => true,
        LpValue::F32(_) => matches!(
            shape.editor,
            ValueEditorHint::Plain
                | ValueEditorHint::Number { .. }
                | ValueEditorHint::Slider { .. }
        ),
        LpValue::String(_) => matches!(shape.editor, ValueEditorHint::Dropdown { .. }),
        _ => false,
    }
}

pub(crate) fn render_slot_edit_status(ui: &mut egui::Ui, status: SlotEditStatus<'_>) {
    if let Some(error) = status.error {
        ui.colored_label(egui::Color32::LIGHT_RED, "rejected")
            .on_hover_text(error);
    }
}

fn render_bool_editor(ui: &mut egui::Ui, value: bool) -> Option<LpValue> {
    let mut edited = value;
    let response = ui.checkbox(&mut edited, "");
    (response.changed() && edited != value).then_some(LpValue::Bool(edited))
}

fn render_f32_editor(ui: &mut egui::Ui, shape: &SlotValueShape, value: f32) -> Option<LpValue> {
    let mut edited = value;
    let response = match &shape.editor {
        ValueEditorHint::Slider { min, max, step } => {
            let mut slider = egui::Slider::new(&mut edited, min.0..=max.0);
            if let Some(step) = step {
                slider = slider
                    .step_by(f64::from(step.0))
                    .fixed_decimals(decimals_for_step(step.0));
            }
            ui.add(slider)
        }
        ValueEditorHint::Number { min, max, step } => {
            let mut drag = egui::DragValue::new(&mut edited);
            if let Some(speed) = step {
                drag = drag.speed(f64::from(speed.0));
            }
            let response = ui.add(drag);
            if let Some(min) = min {
                edited = edited.max(min.0);
            }
            if let Some(max) = max {
                edited = edited.min(max.0);
            }
            response
        }
        ValueEditorHint::Plain => ui.add(egui::DragValue::new(&mut edited)),
        _ => return None,
    };

    let tolerance = f32_edit_tolerance(&shape.editor);
    (response.changed() && response_was_user_edit(&response) && (edited - value).abs() > tolerance)
        .then_some(LpValue::F32(edited))
}

fn render_string_editor(
    ui: &mut egui::Ui,
    root: &str,
    path: &SlotPath,
    shape: &SlotValueShape,
    value: &str,
) -> Option<LpValue> {
    let ValueEditorHint::Dropdown { options } = &shape.editor else {
        return None;
    };

    let mut edited = value.to_string();
    let selected_text = options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.as_str())
        .unwrap_or(value);
    let mut changed = false;
    egui::ComboBox::from_id_salt((root, path))
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            for option in options {
                changed |= ui
                    .selectable_value(&mut edited, option.value.clone(), option.label.as_str())
                    .changed();
            }
        });

    (changed && edited != value).then_some(LpValue::String(edited))
}

fn response_was_user_edit(response: &egui::Response) -> bool {
    response.clicked() || response.dragged()
}

fn f32_edit_tolerance(editor: &ValueEditorHint) -> f32 {
    match editor {
        ValueEditorHint::Slider {
            step: Some(step), ..
        }
        | ValueEditorHint::Number {
            step: Some(step), ..
        } => (step.0.abs() * 0.001).max(0.000_001),
        _ => 0.000_001,
    }
}

fn decimals_for_step(step: f32) -> usize {
    if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else if step >= 0.01 {
        3
    } else {
        4
    }
}

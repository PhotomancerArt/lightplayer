//! Small editable slot-leaf helpers for the temporary debug UI.

use std::collections::BTreeMap;

use eframe::egui;
use lpc_model::{LpValue, SlotPath, SlotPolicy, SlotValueShape, ValueEditorHint};
use lpc_view::SlotMirrorView;
use lpc_wire::{WireSlotMutationId, WireSlotMutationRejection};

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

/// Read-only mutation status lookup for value rows.
pub(crate) struct SlotEditStatusContext<'a> {
    last_mutation_by_slot: &'a BTreeMap<SlotEditKey, WireSlotMutationId>,
    slots: &'a SlotMirrorView,
}

impl<'a> SlotEditStatusContext<'a> {
    pub fn new(
        last_mutation_by_slot: &'a BTreeMap<SlotEditKey, WireSlotMutationId>,
        slots: &'a SlotMirrorView,
    ) -> Self {
        Self {
            last_mutation_by_slot,
            slots,
        }
    }

    pub fn status(&self, root: &str, path: &SlotPath) -> SlotEditStatus<'_> {
        let key = SlotEditKey::new(root, path.clone());
        let Some(id) = self.last_mutation_by_slot.get(&key).copied() else {
            return SlotEditStatus::default();
        };
        SlotEditStatus {
            error: self.slots.error(id),
        }
    }
}

/// Per-row mutation status.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SlotEditStatus<'a> {
    pub error: Option<&'a WireSlotMutationRejection>,
}

/// Render a supported editor for one slot value leaf.
///
/// Returns `Some(value)` only when the user changed a supported writable value.
pub(crate) fn render_slot_value_editor(
    ui: &mut egui::Ui,
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
        _ => false,
    }
}

pub(crate) fn render_slot_edit_status(ui: &mut egui::Ui, status: SlotEditStatus<'_>) {
    if let Some(error) = status.error {
        ui.colored_label(egui::Color32::LIGHT_RED, "rejected")
            .on_hover_text(format!("{error:?}"));
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
                slider = slider.step_by(f64::from(step.0));
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

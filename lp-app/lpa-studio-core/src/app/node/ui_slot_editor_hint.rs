//! Presentation hints for config slot value editors.

/// A selectable value for enum-like or constrained slot editors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotOption {
    /// Serialized or controller-owned value key.
    pub value: String,
    /// Human-readable option label.
    pub label: String,
}

impl UiSlotOption {
    /// Create an option with an explicit value and label.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// A light UI hint for choosing the field component for a slot value.
#[derive(Clone, Debug, PartialEq)]
pub enum UiSlotEditorHint {
    /// Let the renderer choose from the value kind.
    Auto,
    /// Render as a single-line text field.
    Text,
    /// Render as a numeric field with optional constraints.
    Number {
        /// Optional minimum accepted value.
        min: Option<f32>,
        /// Optional maximum accepted value.
        max: Option<f32>,
        /// Optional preferred input step.
        step: Option<f32>,
    },
    /// Render as a slider-like numeric control.
    Slider {
        /// Minimum slider value.
        min: f32,
        /// Maximum slider value.
        max: f32,
        /// Optional preferred slider step.
        step: Option<f32>,
    },
    /// Render as a dropdown using the provided options.
    Dropdown(Vec<UiSlotOption>),
    /// Render a two-dimensional value with an XY affordance.
    Xy,
}

impl UiSlotEditorHint {
    /// Numeric editor with no explicit constraints.
    pub fn number() -> Self {
        Self::Number {
            min: None,
            max: None,
            step: None,
        }
    }

    /// Slider editor with a minimum and maximum.
    pub fn slider(min: f32, max: f32) -> Self {
        Self::Slider {
            min,
            max,
            step: None,
        }
    }

    /// Dropdown editor from `(value, label)` pairs.
    pub fn dropdown(
        options: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::Dropdown(
            options
                .into_iter()
                .map(|(value, label)| UiSlotOption::new(value, label))
                .collect(),
        )
    }
}

impl Default for UiSlotEditorHint {
    fn default() -> Self {
        Self::Auto
    }
}

#[cfg(test)]
mod tests {
    use super::UiSlotEditorHint;

    #[test]
    fn dropdown_collects_options() {
        let hint = UiSlotEditorHint::dropdown([("idle", "Idle"), ("blast", "Blast")]);

        let UiSlotEditorHint::Dropdown(options) = hint else {
            panic!("expected dropdown hint");
        };
        assert_eq!(options[0].value, "idle");
        assert_eq!(options[1].label, "Blast");
    }
}

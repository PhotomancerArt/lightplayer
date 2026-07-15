//! Stable numeric display for live value heroes.
//!
//! Live readings re-render every tick; a naive float print ("400.0" →
//! "400.001") changes width and makes the layout jitter. Fixed decimal
//! places (plus the monospace font every hero already uses) keep the
//! reading still. Non-numeric values pass through untouched.

use lpa_studio_core::UiSlotUnit;

/// Format a live value with a fixed number of decimals when it reads as a
/// finite float; otherwise return it unchanged.
pub(crate) fn fixed_decimal_display(value: &str, unit: Option<&UiSlotUnit>) -> String {
    let trimmed = value.trim();
    let Some(decimals) = fixed_decimal_places(trimmed, unit) else {
        return value.to_string();
    };
    let Ok(number) = trimmed.parse::<f64>() else {
        return value.to_string();
    };
    if number.is_finite() {
        format!("{number:.decimals$}")
    } else {
        value.to_string()
    }
}

fn fixed_decimal_places(value: &str, unit: Option<&UiSlotUnit>) -> Option<usize> {
    if unit.is_some_and(|unit| unit.short == "s" || unit.short == "ms") {
        return Some(3);
    }
    (value.contains('.') || value.contains('e') || value.contains('E')).then_some(3)
}

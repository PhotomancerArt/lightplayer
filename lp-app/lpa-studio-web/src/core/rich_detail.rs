//! Advisory-chip projection for rich-object surfaces.
//!
//! This module was the rich object's DETAIL-POPOVER renderer:
//! `RichDetailSection` turned each core [`RichSection`] into a
//! [`DetailSection`] in fixed schema order (Q4), with `RichWeight::Danger`
//! rendered as the inline red-tinted zone behind a hard separator (Q5).
//! That popover is gone — the card became the control panel at M7′ and
//! then the four-zone roster card — so the renderer and its tone→tint map
//! went with it (ADR `2026-07-17-rich-object-pattern.md`, spike-record
//! note). What survives is the one projection the tab renderer still
//! needs.
//!
//! [`RichSection`]: lpa_studio_core::RichSection
//! [`DetailSection`]: crate::base::DetailSection

use lpa_studio_core::{RichChip, UiStatus, UiStatusKind};

/// Advisory chip → `StatusChip` status (the sim card's tab renderer).
pub(crate) fn chip_status(chip: &RichChip) -> UiStatus {
    match chip.tone {
        UiStatusKind::Neutral => UiStatus::neutral(chip.text.clone()),
        UiStatusKind::Working => UiStatus::working(chip.text.clone()),
        UiStatusKind::Good => UiStatus::good(chip.text.clone()),
        UiStatusKind::Warning => UiStatus::warning(chip.text.clone()),
        UiStatusKind::Attention => UiStatus::attention(chip.text.clone()),
        UiStatusKind::Error => UiStatus::error(chip.text.clone()),
    }
}

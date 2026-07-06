//! Consumer-side rendering of the core [`UiAffordance`] vocabulary.
//!
//! Core computes one affordance per hierarchy surface
//! (`UiAffordance::merged`, priority Error > Unsaved > Live > Busy > Info);
//! this module is the ONE place that turns it into chrome — the detail
//! trigger's glyph + tone (node header, project pane) and the sidebar tree
//! row's small indicator. Status words and dirty counts never render here;
//! they live in the popups.

use lpa_studio_core::{UiAffordance, UiStatusKind};

use crate::app::layout::PaneTone;
use crate::base::{IconMenuTone, StudioIconName};

/// Glyph + trigger tone for one affordance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AffordanceStyle {
    pub icon: StudioIconName,
    pub tone: IconMenuTone,
}

/// The detail-trigger treatment for an affordance: a quiet "i" when there is
/// nothing to announce (OK is not announced — no checkmark, no status
/// coloring), the edit pencil for unsaved (yellow) and live (blue) edits,
/// and the red warning glyph for the attention class.
pub(crate) fn affordance_trigger_style(affordance: UiAffordance) -> AffordanceStyle {
    match affordance {
        UiAffordance::Info => AffordanceStyle {
            icon: StudioIconName::InfoBare,
            tone: IconMenuTone::Quiet,
        },
        UiAffordance::Busy => AffordanceStyle {
            icon: StudioIconName::InfoBare,
            tone: IconMenuTone::Working,
        },
        UiAffordance::Live => AffordanceStyle {
            icon: StudioIconName::Edited,
            tone: IconMenuTone::Live,
        },
        UiAffordance::Unsaved => AffordanceStyle {
            icon: StudioIconName::Edited,
            tone: IconMenuTone::Warning,
        },
        UiAffordance::Error => AffordanceStyle {
            icon: StudioIconName::StepAttention,
            tone: IconMenuTone::Error,
        },
    }
}

/// Header-wash tone for a pane wearing an affordance: announced affordances
/// wash the header in their own tone; a silent (`Info`) pane falls back to
/// its runtime status tone.
pub(crate) fn affordance_pane_tone(affordance: UiAffordance, status: UiStatusKind) -> PaneTone {
    match affordance {
        UiAffordance::Info => status_pane_tone(status),
        UiAffordance::Busy => PaneTone::Working,
        UiAffordance::Live => PaneTone::Live,
        UiAffordance::Unsaved => PaneTone::Warning,
        UiAffordance::Error => PaneTone::Error,
    }
}

/// Foreground color class for the tree row's small affordance indicator;
/// `None` for the silent `Info` affordance (clean rows show nothing).
pub(crate) fn affordance_indicator_class(affordance: UiAffordance) -> Option<&'static str> {
    match affordance {
        UiAffordance::Info => None,
        UiAffordance::Busy => Some(
            "tw:inline-flex tw:h-4 tw:items-center tw:justify-center tw:text-status-working-foreground",
        ),
        UiAffordance::Live => Some(
            "tw:inline-flex tw:h-4 tw:items-center tw:justify-center tw:text-status-live-foreground",
        ),
        UiAffordance::Unsaved => Some(
            "tw:inline-flex tw:h-4 tw:items-center tw:justify-center tw:text-status-warning-foreground",
        ),
        UiAffordance::Error => Some(
            "tw:inline-flex tw:h-4 tw:items-center tw:justify-center tw:text-status-error-foreground",
        ),
    }
}

/// Map a runtime status kind onto the pane's neutral tone vocabulary (the
/// consumer-side mapping required by the pane's layout-only contract).
pub(crate) fn status_pane_tone(kind: UiStatusKind) -> PaneTone {
    match kind {
        UiStatusKind::Neutral => PaneTone::Neutral,
        UiStatusKind::Working => PaneTone::Working,
        UiStatusKind::Good => PaneTone::Good,
        UiStatusKind::Warning => PaneTone::Warning,
        UiStatusKind::Error => PaneTone::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_render_the_confirmed_vocabulary() {
        // OK is not announced: a quiet "i", no status coloring, no checkmark.
        let info = affordance_trigger_style(UiAffordance::Info);
        assert_eq!(info.icon, StudioIconName::InfoBare);
        assert_eq!(info.tone, IconMenuTone::Quiet);

        // Genuine activity keeps the "i" glyph in the working tone.
        let busy = affordance_trigger_style(UiAffordance::Busy);
        assert_eq!(busy.icon, StudioIconName::InfoBare);
        assert_eq!(busy.tone, IconMenuTone::Working);

        // Edits wear the pencil: yellow for unsaved, blue for live.
        let unsaved = affordance_trigger_style(UiAffordance::Unsaved);
        assert_eq!(unsaved.icon, StudioIconName::Edited);
        assert_eq!(unsaved.tone, IconMenuTone::Warning);
        let live = affordance_trigger_style(UiAffordance::Live);
        assert_eq!(live.icon, StudioIconName::Edited);
        assert_eq!(live.tone, IconMenuTone::Live);

        // Attention: the red warning glyph.
        let error = affordance_trigger_style(UiAffordance::Error);
        assert_eq!(error.icon, StudioIconName::StepAttention);
        assert_eq!(error.tone, IconMenuTone::Error);
    }

    #[test]
    fn pane_tone_follows_the_affordance_and_falls_back_to_status() {
        assert_eq!(
            affordance_pane_tone(UiAffordance::Unsaved, UiStatusKind::Good),
            PaneTone::Warning
        );
        assert_eq!(
            affordance_pane_tone(UiAffordance::Live, UiStatusKind::Good),
            PaneTone::Live
        );
        assert_eq!(
            affordance_pane_tone(UiAffordance::Error, UiStatusKind::Good),
            PaneTone::Error
        );
        assert_eq!(
            affordance_pane_tone(UiAffordance::Busy, UiStatusKind::Working),
            PaneTone::Working
        );
        // A silent pane keeps its runtime status tone on the header wash.
        assert_eq!(
            affordance_pane_tone(UiAffordance::Info, UiStatusKind::Good),
            PaneTone::Good
        );
        assert_eq!(
            affordance_pane_tone(UiAffordance::Info, UiStatusKind::Neutral),
            PaneTone::Neutral
        );
    }

    #[test]
    fn only_announced_affordances_get_a_tree_indicator() {
        assert!(affordance_indicator_class(UiAffordance::Info).is_none());
        for affordance in [
            UiAffordance::Busy,
            UiAffordance::Live,
            UiAffordance::Unsaved,
            UiAffordance::Error,
        ] {
            assert!(affordance_indicator_class(affordance).is_some());
        }
    }
}

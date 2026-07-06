//! Project header on the shared pane grammar (D4/D5): the dissolved M2 save
//! strip.
//!
//! A header-only `StudioPane` at the top of the project sidebar: title =
//! project name, a persistent state chip ("unchanged" is a visible state),
//! contextual Save / Revert-to-saved icon actions supplied by the controller
//! (`ProjectEditorView.header_actions`), and a `DetailPopover` carrying the
//! old strip's popup content — overlay revision, awaiting-ack, and the
//! unsaved/live sections with their status tints. The popup deliberately
//! stays at this detail level; the full "what changed" panel is M3.

use dioxus::prelude::*;
use lpa_studio_core::{DirtySummary, UiAction, UiPaneAction};

use crate::app::layout::{PaneChip, PaneChrome, PaneTone, StudioPane};
use crate::base::{
    DetailPopover, DetailSectionTint, IconMenuTone, PopoverPlacement, StudioIconName,
    detail_popover_section_class,
};

/// Overall overlay state summarized by the header's detail trigger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectHeaderState {
    /// No pending persisted edits; nothing for Save to write.
    Unchanged,
    /// Persisted edits are pending in the overlay, not yet saved.
    Uncommitted,
    /// An edit or save operation is awaiting its server acknowledgement.
    InProgress,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectHeader(
    /// Project name shown as the pane title.
    project_id: String,
    dirty: DirtySummary,
    overlay_revision: i64,
    edits_in_flight: usize,
    /// Controller-supplied contextual actions (Save / Revert to saved);
    /// empty unless persisted edits are pending.
    actions: Vec<UiPaneAction>,
    on_action: EventHandler<UiAction>,
    /// Open the detail popup immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    let state = project_header_state(&dirty, edits_in_flight);
    let chrome = PaneChrome {
        tone: project_header_tone(&dirty, edits_in_flight),
        accent: false,
        chips: project_header_chips(&dirty, edits_in_flight),
    };

    rsx! {
        StudioPane {
            title: project_id,
            kind: "Project".to_string(),
            chrome,
            actions,
            on_action,
            detail: rsx! {
                ProjectDetailPopover {
                    state,
                    dirty,
                    overlay_revision,
                    edits_in_flight,
                    initially_open,
                }
            },
        }
    }
}

/// The detail popup on the shared [`DetailPopover`] base: overlay revision,
/// awaiting-ack, and the unsaved/live sections with their status tints.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetailPopover(
    state: ProjectHeaderState,
    dirty: DirtySummary,
    overlay_revision: i64,
    edits_in_flight: usize,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let (icon, tone, label) = match state {
        ProjectHeaderState::Unchanged => (
            StudioIconName::StepComplete,
            IconMenuTone::Quiet,
            "No unsaved project changes",
        ),
        ProjectHeaderState::Uncommitted => (
            StudioIconName::Edited,
            IconMenuTone::Warning,
            "Project has unsaved changes",
        ),
        ProjectHeaderState::InProgress => (
            StudioIconName::StatusRunning,
            IconMenuTone::Working,
            "Edit in progress",
        ),
    };

    rsx! {
        DetailPopover {
            icon,
            label: label.to_string(),
            tone,
            placement: PopoverPlacement::BottomEnd,
            active: state != ProjectHeaderState::Unchanged,
            initially_open,
            section { class: "tw:grid tw:gap-1 tw:px-3 tw:py-2",
                h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Pending edits" }
                ProjectDetailRow { label: "State", value: state_label(state).to_string() }
                ProjectDetailRow { label: "Overlay revision", value: overlay_revision.to_string() }
                if edits_in_flight > 0 {
                    ProjectDetailRow { label: "Awaiting ack", value: edits_in_flight.to_string() }
                }
            }
            section { class: detail_popover_section_class(unsaved_section_tint(&dirty)),
                ProjectDetailRow { label: "Unsaved (persisted)", value: dirty.persisted.to_string() }
            }
            section { class: detail_popover_section_class(live_section_tint(&dirty)),
                ProjectDetailRow { label: "Live (transient)", value: dirty.transient.to_string() }
                p { class: "tw:m-0 tw:pt-1 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                    "Live controls apply to the running project and are never written by Save."
                }
            }
            if dirty.failed > 0 {
                section { class: detail_popover_section_class(DetailSectionTint::Error),
                    ProjectDetailRow { label: "Failed edits", value: dirty.failed.to_string() }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetailRow(label: &'static str, value: String) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:flex tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
            span { class: "tw:font-bold tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono tw:text-muted-foreground", "{value}" }
        }
    }
}

fn project_header_state(dirty: &DirtySummary, edits_in_flight: usize) -> ProjectHeaderState {
    if edits_in_flight > 0 {
        ProjectHeaderState::InProgress
    } else if dirty.persisted > 0 {
        ProjectHeaderState::Uncommitted
    } else {
        ProjectHeaderState::Unchanged
    }
}

fn state_label(state: ProjectHeaderState) -> &'static str {
    match state {
        ProjectHeaderState::Unchanged => "unchanged",
        ProjectHeaderState::Uncommitted => "uncommitted",
        ProjectHeaderState::InProgress => "in progress",
    }
}

/// Header tone: failed edits wash red, an in-flight op washes working,
/// otherwise the dominant dirty bucket (unsaved > live, per D6); a clean
/// idle project keeps the neutral header.
fn project_header_tone(dirty: &DirtySummary, edits_in_flight: usize) -> PaneTone {
    if dirty.failed > 0 {
        PaneTone::Error
    } else if edits_in_flight > 0 {
        PaneTone::Working
    } else if dirty.persisted > 0 {
        PaneTone::Warning
    } else if dirty.transient > 0 {
        PaneTone::Live
    } else {
        PaneTone::Neutral
    }
}

/// Persistent state chips (D4: the chip is always present — "unchanged" is a
/// visible state). A dirty project shows the per-bucket chips (D6: yellow =
/// unsaved, blue = live, red = failed), preceded by a working "syncing" chip
/// while edits await their server acknowledgement.
fn project_header_chips(dirty: &DirtySummary, edits_in_flight: usize) -> Vec<PaneChip> {
    let mut chips = Vec::new();
    if edits_in_flight > 0 {
        chips.push(PaneChip {
            tone: PaneTone::Working,
            text: "syncing".to_string(),
            title: format!("{edits_in_flight} edits awaiting server acknowledgement"),
        });
    }
    if dirty.persisted > 0 {
        chips.push(PaneChip {
            tone: PaneTone::Warning,
            text: format!("{} unsaved", dirty.persisted),
            title: "Pending persisted edits Save will write".to_string(),
        });
    }
    if dirty.transient > 0 {
        chips.push(PaneChip {
            tone: PaneTone::Live,
            text: format!("{} live", dirty.transient),
            title: "Touched live controls; never written by Save".to_string(),
        });
    }
    if dirty.failed > 0 {
        chips.push(PaneChip {
            tone: PaneTone::Error,
            text: format!("{} failed", dirty.failed),
            title: "Failed edits need attention".to_string(),
        });
    }
    if chips.is_empty() {
        chips.push(PaneChip {
            tone: PaneTone::Neutral,
            text: "unchanged".to_string(),
            title: "No pending project edits".to_string(),
        });
    }
    chips
}

/// The unsaved section wears the warning (yellow) edited tint whenever
/// persisted edits are pending — the same treatment as edited slot rows.
fn unsaved_section_tint(dirty: &DirtySummary) -> DetailSectionTint {
    if dirty.persisted > 0 {
        DetailSectionTint::Warning
    } else {
        DetailSectionTint::None
    }
}

/// The live section wears the dedicated live (blue) tint whenever transient
/// controls are touched — matching live slot rows, distinct from the yellow
/// unsaved treatment.
fn live_section_tint(dirty: &DirtySummary) -> DetailSectionTint {
    if dirty.transient > 0 {
        DetailSectionTint::Live
    } else {
        DetailSectionTint::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty(persisted: usize, transient: usize, failed: usize) -> DirtySummary {
        DirtySummary {
            persisted,
            transient,
            failed,
        }
    }

    #[test]
    fn state_tracks_in_flight_then_persisted() {
        assert_eq!(
            project_header_state(&dirty(0, 0, 0), 0),
            ProjectHeaderState::Unchanged
        );
        assert_eq!(
            project_header_state(&dirty(0, 2, 0), 0),
            ProjectHeaderState::Unchanged
        );
        assert_eq!(
            project_header_state(&dirty(1, 0, 0), 0),
            ProjectHeaderState::Uncommitted
        );
        assert_eq!(
            project_header_state(&dirty(1, 0, 0), 1),
            ProjectHeaderState::InProgress
        );
    }

    #[test]
    fn clean_idle_project_shows_the_visible_unchanged_chip() {
        let chips = project_header_chips(&DirtySummary::clean(), 0);
        assert_eq!(chips.len(), 1);
        assert_eq!(chips[0].tone, PaneTone::Neutral);
        assert_eq!(chips[0].text, "unchanged");
        assert_eq!(
            project_header_tone(&DirtySummary::clean(), 0),
            PaneTone::Neutral
        );
    }

    #[test]
    fn dirty_chips_cover_each_nonzero_bucket_and_replace_unchanged() {
        let chips = project_header_chips(&dirty(3, 2, 1), 0);
        assert_eq!(chips.len(), 3);
        assert_eq!(chips[0].tone, PaneTone::Warning);
        assert_eq!(chips[0].text, "3 unsaved");
        assert_eq!(chips[1].tone, PaneTone::Live);
        assert_eq!(chips[1].text, "2 live");
        assert_eq!(chips[2].tone, PaneTone::Error);
        assert_eq!(chips[2].text, "1 failed");
        assert!(!chips.iter().any(|chip| chip.text == "unchanged"));
    }

    #[test]
    fn in_flight_edits_prepend_the_syncing_chip_and_working_tone() {
        let chips = project_header_chips(&dirty(1, 0, 0), 2);
        assert_eq!(chips[0].tone, PaneTone::Working);
        assert_eq!(chips[0].text, "syncing");
        assert_eq!(chips[1].text, "1 unsaved");
        assert_eq!(project_header_tone(&dirty(1, 0, 0), 2), PaneTone::Working);
        // Failed edits still dominate the wash.
        assert_eq!(project_header_tone(&dirty(1, 0, 1), 2), PaneTone::Error);
    }

    #[test]
    fn header_tone_prefers_unsaved_over_live() {
        assert_eq!(project_header_tone(&dirty(2, 1, 0), 0), PaneTone::Warning);
        assert_eq!(project_header_tone(&dirty(0, 1, 0), 0), PaneTone::Live);
    }

    #[test]
    fn popup_sections_tint_only_their_dirty_bucket() {
        assert_eq!(
            unsaved_section_tint(&dirty(1, 0, 0)),
            DetailSectionTint::Warning
        );
        assert_eq!(
            unsaved_section_tint(&dirty(0, 2, 0)),
            DetailSectionTint::None
        );
        assert_eq!(live_section_tint(&dirty(0, 2, 0)), DetailSectionTint::Live);
        assert_eq!(live_section_tint(&dirty(1, 0, 0)), DetailSectionTint::None);
    }
}

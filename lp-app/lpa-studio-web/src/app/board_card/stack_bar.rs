//! [`StackBar`]: one of the card's five bars, one 28 px row — its icon, its
//! summary, its aside and its action flush at the end; while it carries work,
//! the work instead ([`BarWork`]).
//!
//! The icon, summary and aside are ONE full-width button: the trigger of the
//! bar's details ([`BarDetails`], the merged detail card). The action sits
//! outside the trigger (DC14), so pressing it never opens the details.
//!
//! The bar's tone tints the row in its status family with the tone's icon
//! leading — never colour alone (`docs/style/ui.md` "Icons"): Attention,
//! Warning, Error (striped), Live (blue: Update), Good. Neutral is untinted.
//! Which tone, which words and which offer are core's; this file only says
//! how each looks.

use dioxus::prelude::*;
use lpa_studio_core::{BarWorkState, OfferPath, UiAction, UiStackBar, UiStatusKind};

use super::bar_details::BarDetails;
use super::bar_work::{BarWork, work_state_name};
use super::card_action::{CardAction, CardActionLook, OfferAction};
use crate::base::{PopoverPlacement, StudioIcon, StudioIconName, action_icon_name};
use crate::core::ActionButtonVariant;

/// One bar. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StackBar(
    bar: UiStackBar,
    /// `devices/<board ref>`: the board the bar's panels act on.
    board: OfferPath,
    /// Stories: mount the bar's details open.
    #[props(default)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let layer = bar.layer.as_str();
    // Work tints the row by how it stands; otherwise the bar's own tone.
    let tone = match bar.work.as_ref().map(|work| &work.state) {
        Some(BarWorkState::Running) => UiStatusKind::Neutral,
        Some(BarWorkState::Done) => UiStatusKind::Good,
        Some(BarWorkState::Failed { .. }) => UiStatusKind::Error,
        None => bar.tone,
    };
    let work_state = bar.work.as_ref().map(|work| work_state_name(&work.state));
    let title = bar_title(&bar);
    let label = format!("{} details", layer_name(bar.layer));
    let details = bar.details.clone();
    let trigger = match bar.work.clone() {
        Some(work) => rsx! {
            BarWork { work }
        },
        None => bar_line(&bar),
    };
    // Outside the trigger: the work's own way out (Cancel, Retry), else the
    // bar's action. A bar with neither ends in its details' chevron.
    let end = match bar.work.as_ref() {
        Some(work) => match &work.state {
            BarWorkState::Running => work.cancel.clone().map(|cancel| {
                rsx! {
                    OfferAction { path: cancel, variant: ActionButtonVariant::RowEnd, on_action }
                }
            }),
            BarWorkState::Failed { retry: Some(retry) } => Some(rsx! {
                CardAction { action: retry.clone(), look: CardActionLook::BarEnd, on_action }
            }),
            BarWorkState::Failed { retry: None } | BarWorkState::Done => None,
        },
        None => bar.action.clone().map(|action| {
            rsx! {
                CardAction { action, look: CardActionLook::BarEnd, on_action }
            }
        }),
    };
    let foot = bar.work.as_ref().and_then(|work| match work.state {
        BarWorkState::Running => Some((work.percent, work.other_device)),
        BarWorkState::Done | BarWorkState::Failed { .. } => None,
    });
    rsx! {
        div {
            class: bar_class(tone),
            "data-bar": "{layer}",
            "data-bar-work": work_state,
            div { class: TRIGGER_SLOT_CLASS,
                BarDetails {
                    sections: details.sections,
                    panels: details.panels,
                    raised: details.raised,
                    board,
                    label,
                    title,
                    trigger: rsx! {
                        {trigger}
                        if end.is_none() {
                            span { class: CHEVRON_CLASS, aria_hidden: "true",
                                StudioIcon { name: StudioIconName::Collapsed, size: 12 }
                            }
                        }
                    },
                    trigger_class: TRIGGER_CLASS.to_string(),
                    trigger_open_class: TRIGGER_OPEN_CLASS.to_string(),
                    placement: PopoverPlacement::BottomStart,
                    initially_open,
                    on_action,
                }
            }
            {end}
            if let Some((percent, other)) = foot {
                super::bar_work::WorkFoot { percent, other }
            }
        }
    }
}

/// The bar's resting line: the leading icon (the tone's, on a tinted bar),
/// the summary, the aside.
fn bar_line(bar: &UiStackBar) -> Element {
    let toned = bar.tone != UiStatusKind::Neutral;
    let icon = tone_icon(bar.tone).or_else(|| action_icon_name(Some(bar.icon.as_str())));
    let aside_icon = action_icon_name(bar.aside_icon.as_deref());
    rsx! {
        if let Some(icon) = icon {
            span { class: if toned { ICON_TONED_CLASS } else { ICON_CLASS }, aria_hidden: "true",
                StudioIcon { name: icon, size: 12 }
            }
        }
        span { class: if toned { SUMMARY_TONED_CLASS } else { SUMMARY_CLASS }, "{bar.summary}" }
        if let Some(aside) = bar.aside.clone() {
            span { class: if toned { ASIDE_TONED_CLASS } else { ASIDE_CLASS },
                if let Some(icon) = aside_icon {
                    span { class: "tw:inline-flex tw:flex-none", aria_hidden: "true",
                        StudioIcon { name: icon, size: 11 }
                    }
                }
                span { class: "tw:truncate", "{aside}" }
            }
        }
    }
}

/// The bar's hover title: its whole line, which the row may cut.
fn bar_title(bar: &UiStackBar) -> String {
    match (&bar.work, &bar.aside) {
        (Some(work), _) => work.words.clone(),
        (None, Some(aside)) => format!("{} · {aside}", bar.summary),
        (None, None) => bar.summary.clone(),
    }
}

/// The bar's name, for its details' accessible name.
fn layer_name(layer: lpa_studio_core::BarLayer) -> &'static str {
    match layer {
        lpa_studio_core::BarLayer::Project => "Project",
        lpa_studio_core::BarLayer::Connection => "Connection",
        lpa_studio_core::BarLayer::Access => "Access",
        lpa_studio_core::BarLayer::Firmware => "Firmware",
        lpa_studio_core::BarLayer::Hardware => "Hardware",
    }
}

/// The row's classes in `tone`: the fixed 28 px row and its hairline, tinted
/// in the tone's family (border, ground, ink, and the edge its flush action
/// draws). Neutral is the plain row.
pub(crate) fn bar_class(tone: UiStatusKind) -> String {
    format!("{BAR_BASE_CLASS} {}", bar_tone_class(tone))
}

/// The tint part of [`bar_class`].
pub(crate) fn bar_tone_class(tone: UiStatusKind) -> &'static str {
    match tone {
        UiStatusKind::Neutral => "tw:border-border tw:text-muted-foreground",
        UiStatusKind::Working => {
            "tw:border-status-working-border tw:bg-status-working-bg tw:text-status-working-foreground tw:[--ux-row-flush-edge:var(--studio-status-working-border)]"
        }
        UiStatusKind::Good => {
            "tw:border-status-good-border tw:bg-status-good-bg tw:text-status-good-foreground tw:[--ux-row-flush-edge:var(--studio-status-good-border)]"
        }
        UiStatusKind::Live => {
            "tw:border-status-live-border tw:bg-status-live-bg tw:text-status-live-foreground tw:[--ux-row-flush-edge:var(--studio-status-live-border)]"
        }
        UiStatusKind::Warning => {
            "tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground tw:[--ux-row-flush-edge:var(--studio-status-warning-border)]"
        }
        UiStatusKind::Attention => {
            "tw:border-status-attention-border tw:bg-status-attention-bg tw:text-status-attention-foreground tw:[--ux-row-flush-edge:var(--studio-status-attention-border)]"
        }
        // Stripes mark a failure (ui.md): laid over the error ground.
        UiStatusKind::Error => {
            "tw:border-status-error-border tw:bg-status-error-bg tw:bg-[image:var(--studio-status-error-stripes)] tw:text-status-error-foreground tw:[--ux-row-flush-edge:var(--studio-status-error-border)]"
        }
    }
}

/// The icon a tinted bar leads with, so its tone never rests on colour
/// alone; `None` for Neutral (the bar keeps its own icon).
pub(crate) fn tone_icon(tone: UiStatusKind) -> Option<StudioIconName> {
    match tone {
        UiStatusKind::Neutral => None,
        UiStatusKind::Working => Some(StudioIconName::Refresh),
        UiStatusKind::Good => Some(StudioIconName::StepComplete),
        UiStatusKind::Live => Some(StudioIconName::Newer),
        UiStatusKind::Warning | UiStatusKind::Attention => Some(StudioIconName::StepAttention),
        UiStatusKind::Error => Some(StudioIconName::StatusError),
    }
}

/// The fixed row: 28 px with its top hairline (the bars are full-bleed rows
/// of one card, never boxes), the last one rounded with the card.
const BAR_BASE_CLASS: &str = "ux-board-bar tw:relative tw:flex tw:h-7 tw:min-w-0 tw:items-stretch tw:border-0 tw:border-t tw:border-solid tw:text-[11.5px] tw:last:rounded-b-[7px]";

/// The trigger's slot: the rest of the row, its popover wrapper stretched to
/// it (the wrapper is an inline grid that centres its button). Positioned:
/// a pick the details hand back opens over it.
const TRIGGER_SLOT_CLASS: &str = "tw:relative tw:grid tw:min-w-0 tw:flex-1 tw:[&>span]:h-full tw:[&>span]:w-full tw:[&>span]:place-items-stretch";

/// The trigger: the row's icon, summary and aside as one button, the row's
/// own ink. Tailwind preflight is not loaded, so the UA chrome is reset.
const TRIGGER_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:px-2.5 tw:text-left tw:text-[11.5px] tw:text-inherit tw:transition-colors tw:hover:bg-white/5 ux-focus-ring";

/// …while its details are open.
const TRIGGER_OPEN_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-white/5 tw:px-2.5 tw:text-left tw:text-[11.5px] tw:text-inherit ux-focus-ring";

const ICON_CLASS: &str = "tw:inline-flex tw:flex-none tw:text-subtle-foreground";
const ICON_TONED_CLASS: &str = "tw:inline-flex tw:flex-none";
const SUMMARY_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:truncate tw:text-foreground";
const SUMMARY_TONED_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:truncate tw:font-bold";
const ASIDE_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:max-w-[45%] tw:flex-none tw:items-center tw:gap-1 tw:text-subtle-foreground";
const ASIDE_TONED_CLASS: &str =
    "tw:inline-flex tw:min-w-0 tw:max-w-[45%] tw:flex-none tw:items-center tw:gap-1 tw:opacity-80";
const CHEVRON_CLASS: &str = "tw:inline-flex tw:flex-none tw:text-dim-foreground";

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [UiStatusKind; 7] = [
        UiStatusKind::Neutral,
        UiStatusKind::Working,
        UiStatusKind::Good,
        UiStatusKind::Live,
        UiStatusKind::Warning,
        UiStatusKind::Attention,
        UiStatusKind::Error,
    ];

    /// Every tone maps to a tint in its own family — Neutral to none — and
    /// a tinted bar carries its tone's icon (never colour alone).
    #[test]
    fn every_tone_tints_in_its_family_and_carries_its_icon() {
        for kind in KINDS {
            let class = bar_tone_class(kind);
            match kind {
                UiStatusKind::Neutral => {
                    assert!(!class.contains("tw:bg-"), "{class}");
                    assert!(!class.contains("status"), "{class}");
                    assert_eq!(tone_icon(kind), None);
                }
                _ => {
                    let family = format!("{kind:?}").to_lowercase();
                    for part in ["border", "bg", "foreground"] {
                        let token = format!("status-{family}-{part}");
                        assert!(class.contains(&token), "{token} missing: {class}");
                    }
                    assert!(tone_icon(kind).is_some(), "{kind:?} has no icon");
                }
            }
        }
        // Stripes are a failure's alone.
        assert!(bar_tone_class(UiStatusKind::Error).contains("error-stripes"));
        for kind in KINDS
            .into_iter()
            .filter(|kind| *kind != UiStatusKind::Error)
        {
            assert!(!bar_tone_class(kind).contains("stripes"), "{kind:?}");
        }
    }

    /// A tinted bar keeps its height: the row is the same fixed 28 px in
    /// every tone, one line, never wrapping.
    #[test]
    fn a_tinted_bar_keeps_its_height() {
        for kind in KINDS {
            let class = bar_class(kind);
            assert!(class.contains("tw:h-7"), "{class}");
            assert!(!class.contains("min-h"), "{class}");
            assert!(!class.contains("tw:py-"), "{class}");
        }
        for class in [SUMMARY_CLASS, SUMMARY_TONED_CLASS] {
            assert!(class.contains("tw:truncate"), "{class}");
        }
        assert!(TRIGGER_CLASS.contains("tw:h-full"));
        assert!(TRIGGER_OPEN_CLASS.contains("tw:h-full"));
    }

    /// The bars' hairlines: every bar carries the full-bleed top border,
    /// and nothing inside a bar draws a frame of its own.
    #[test]
    fn every_bar_carries_the_full_bleed_hairline() {
        assert!(BAR_BASE_CLASS.contains("tw:border-t tw:border-solid"));
        assert!(
            !BAR_BASE_CLASS.contains("tw:px-"),
            "the row pads its pieces, not itself"
        );
        for class in [TRIGGER_CLASS, ICON_CLASS, SUMMARY_CLASS, ASIDE_CLASS] {
            assert!(!class.contains("rounded"), "{class}");
            assert!(!class.contains("tw:border "), "{class}");
        }
    }
}

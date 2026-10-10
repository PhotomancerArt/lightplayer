//! [`BarWork`]: work in progress, in the bar doing it (D34) — the bar's
//! content while it carries [`UiBarWork`]: Studio's conic spinner and the
//! step and percent while it runs, with the iridescent fill along the
//! bar's foot ([`WorkFoot`]: its width the percent, a sweep when the
//! percent is unknown, quieter when another device runs it); the check and
//! the words, green, when it ended well (core keeps it there for a few
//! seconds); the failure's mark and words, striped, when it failed. The
//! bar's row draws the tint ([`super::stack_bar::bar_class`]) and the
//! work's Cancel or Retry beside it. The picture and the status corner
//! never change for work.

use dioxus::prelude::*;
use lpa_studio_core::{BarWorkState, UiBarWork};

use crate::base::{
    StudioIcon, StudioIconName, conic_spinner_class, iridescent_fill_class,
    iridescent_fill_static_class,
};

/// The work, as the bar's content. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BarWork(work: UiBarWork) -> Element {
    let mark = match work.state {
        BarWorkState::Running => rsx! {
            span { class: "{conic_spinner_class()} ux-board-spinner", aria_hidden: "true" }
        },
        BarWorkState::Done => rsx! {
            span { class: "tw:inline-flex tw:flex-none", aria_hidden: "true",
                StudioIcon { name: StudioIconName::StepComplete, size: 12 }
            }
        },
        BarWorkState::Failed { .. } => rsx! {
            span { class: "tw:inline-flex tw:flex-none", aria_hidden: "true",
                StudioIcon { name: StudioIconName::StatusError, size: 12 }
            }
        },
    };
    rsx! {
        {mark}
        span { class: words_class(&work.state), "{work.words}" }
    }
}

/// The running work's fill along the bar's foot: absolutely placed, so it
/// costs the row no height.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn WorkFoot(percent: Option<u8>, other: bool) -> Element {
    rsx! {
        div {
            class: foot_class(percent, other),
            style: foot_style(percent),
            aria_hidden: "true",
        }
    }
}

/// The hook's word for where the work stands (`data-bar-work`).
pub(crate) fn work_state_name(state: &BarWorkState) -> &'static str {
    match state {
        BarWorkState::Running => "running",
        BarWorkState::Done => "done",
        BarWorkState::Failed { .. } => "failed",
    }
}

/// The words: strong while running (the row is neutral), the row's own
/// tinted ink once it ended. One line, cut, never wrapped.
fn words_class(state: &BarWorkState) -> &'static str {
    match state {
        BarWorkState::Running => {
            "tw:min-w-0 tw:flex-1 tw:truncate tw:font-bold tw:text-strong-foreground"
        }
        BarWorkState::Done | BarWorkState::Failed { .. } => {
            "tw:min-w-0 tw:flex-1 tw:truncate tw:font-bold"
        }
    }
}

/// The foot's classes: the iridescent paint at the percent, or the sweep
/// (the paint alone, with its own motion — two `animation`s on one element
/// is one too many) when the work cannot say how far along it is; quieter
/// for another device's work.
pub(crate) fn foot_class(percent: Option<u8>, other: bool) -> String {
    let paint = match percent {
        Some(_) => format!("{} {FOOT_CLASS}", iridescent_fill_class()),
        None => format!(
            "{} {FOOT_CLASS} {FOOT_SWEEP_CLASS}",
            iridescent_fill_static_class()
        ),
    };
    match other {
        true => format!("{paint} {FOOT_OTHER_CLASS}"),
        false => paint,
    }
}

/// The foot's width, set in every state: a whole-string `style` write never
/// removes a property, so a sweep that follows a measured fill would keep
/// the old width unless this says the sweep's own.
pub(crate) fn foot_style(percent: Option<u8>) -> String {
    match percent {
        Some(percent) => format!("width: {}%;", u32::from(percent).min(100)),
        None => "width: 35%;".to_string(),
    }
}

/// Along the row's foot, 2 px, under everything the row holds.
const FOOT_CLASS: &str =
    "tw:pointer-events-none tw:absolute tw:bottom-0 tw:left-0 tw:h-0.5 tw:rounded-r-xs";

/// The sweep: the paint shuttling across while it shifts.
const FOOT_SWEEP_CLASS: &str = "tw:[animation:ux-iri-sweep_4s_linear_infinite,ux-card-op-sweep_1.1s_ease-in-out_infinite] tw:motion-reduce:[animation:none]";

/// Another device's work: the quieter fill.
const FOOT_OTHER_CLASS: &str = "tw:opacity-50";

#[cfg(test)]
mod tests {
    use super::*;

    /// The foot always says its width, so a sweep after a measured fill
    /// never keeps the old one; another device's fill is quieter.
    #[test]
    fn the_fill_states_its_width_in_every_state() {
        assert_eq!(foot_style(Some(40)), "width: 40%;");
        assert_eq!(foot_style(Some(250)), "width: 100%;");
        assert_eq!(foot_style(None), "width: 35%;");
        assert!(foot_class(Some(40), true).contains("tw:opacity-50"));
        assert!(!foot_class(Some(40), false).contains("opacity"));
        assert!(foot_class(Some(40), false).contains("ux-iri-fill"));
        // The sweep carries its own motion over the static paint.
        let sweep = foot_class(None, false);
        assert!(sweep.contains("ux-iri-fill-static"), "{sweep}");
        assert!(sweep.contains("ux-card-op-sweep"), "{sweep}");
    }

    /// A bar with work is the same one fixed row in every state: the foot
    /// is absolutely placed, and the words are one cut line.
    #[test]
    fn the_work_is_one_fixed_row_in_every_state() {
        assert!(FOOT_CLASS.contains("tw:absolute"));
        for state in [
            BarWorkState::Running,
            BarWorkState::Done,
            BarWorkState::Failed { retry: None },
        ] {
            let class = words_class(&state);
            assert!(class.contains("tw:truncate"), "{class}");
            assert!(!class.contains("wrap"), "{class}");
        }
        assert_eq!(work_state_name(&BarWorkState::Running), "running");
        assert_eq!(work_state_name(&BarWorkState::Done), "done");
        assert_eq!(
            work_state_name(&BarWorkState::Failed { retry: None }),
            "failed"
        );
    }
}

//! [`StackBar`]: one of the card's five rows, one 28 px line in the card's
//! one grammar (`docs/ux/anatomy/board-card.md`, "The row"): **icon · kind
//! · name · state · action**. The icon is the kind's own in every state;
//! the kind is a fixed muted column, so the names line up down the card;
//! the name is the thing itself; the state follows only when it is not
//! normal; the action sits flush at the end. The name is the row's value
//! and keeps its width: where the row is short of room the state gives
//! way first — cut at its end, then gone — before the name is cut
//! ([`aside_style`]). While the row works, the
//! conic spinner takes the icon's place and the work's words are its state
//! ([`row_words`]), with the iridescent fill along its foot
//! ([`WorkFoot`](super::bar_work::WorkFoot)).
//!
//! The icon, kind, name and state are ONE full-width button: the trigger of
//! the row's details ([`BarDetails`], the merged detail card). The action
//! and its way out sit outside the trigger (DC14), so pressing one never
//! opens the details; a row whose end only opens its details ("Review")
//! draws that section inside the trigger, because that is all it does.
//!
//! A mark ([`UiBarMark`]) is an icon alone at the line's end, after the
//! state — the cloud on a board also reachable through lightplayer.app —
//! with its words as its accessible name and hover: filled in the live tone
//! while it holds, struck through while it doesn't (never colour alone).
//!
//! The row's tone tints it in its status family (`docs/style/ui.md`):
//! Attention (a device needs you), Error (broken, striped), Good (work that
//! just went well); the kind takes the row's own colour, quieter. A stale
//! row (last known) is dimmed, its words unchanged. Which tone, which words
//! and which offer are core's; this file only says how each looks.

use dioxus::prelude::*;
use lpa_studio_core::{BarWorkState, OfferPath, UiAction, UiBarMark, UiStackBar, UiStatusKind};

use super::bar_details::BarDetails;
use super::bar_work::work_state_name;
use super::card_action::{CardAction, CardActionLook, OfferAction};
use crate::base::{
    PopoverPlacement, StudioIcon, StudioIconName, action_icon_name, conic_spinner_class,
};
use crate::core::ActionButtonVariant;

/// One row. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StackBar(
    bar: UiStackBar,
    /// `devices/<board ref>`: the board the row's panels act on.
    board: OfferPath,
    /// Stories: mount the row's details open.
    #[props(default)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let layer = bar.layer.as_str();
    let tone = row_tone(&bar);
    let work_state = bar.work.as_ref().map(|work| work_state_name(&work.state));
    let title = bar_title(&bar);
    let label = format!("{} details", layer_name(bar.layer));
    let details = bar.details.clone();
    let trigger = bar_line(&bar);
    // Outside the trigger: the work's own way out (Cancel, Retry), else the
    // row's action and its way out. A row with none of them, and no section
    // of its own that opens its details, ends in the details' chevron.
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
        None => match (bar.action.clone(), bar.way_out.clone()) {
            (None, None) => None,
            (action, way_out) => Some(rsx! {
                if let Some(action) = action {
                    CardAction { action, look: CardActionLook::BarEnd, on_action }
                }
                if let Some(way_out) = way_out {
                    CardAction { action: way_out, look: CardActionLook::BarEnd, on_action }
                }
            }),
        },
    };
    let chevron = end.is_none() && opens_details(&bar).is_none();
    let foot = bar.work.as_ref().and_then(|work| match work.state {
        BarWorkState::Running => Some((work.percent, work.other_device)),
        BarWorkState::Done | BarWorkState::Failed { .. } => None,
    });
    rsx! {
        div {
            class: bar_class(tone),
            "data-bar": "{layer}",
            "data-bar-work": work_state,
            "data-stale": if bar.stale { "true" },
            div { class: TRIGGER_SLOT_CLASS,
                BarDetails {
                    sections: details.sections,
                    panels: details.panels,
                    foot: details.foot,
                    raised: details.raised,
                    board,
                    label,
                    title,
                    trigger: rsx! {
                        {trigger}
                        if chevron {
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

/// The tone the row is drawn in: neutral while its work runs; otherwise the
/// row's own, which core decides — green a moment after work that went
/// well, red after work that failed, and quiet when the card's surface says
/// the row's problem (the rows go quiet).
pub(crate) fn row_tone(bar: &UiStackBar) -> UiStatusKind {
    match bar.work.as_ref().map(|work| &work.state) {
        Some(BarWorkState::Running) => UiStatusKind::Neutral,
        Some(BarWorkState::Done | BarWorkState::Failed { .. }) | None => bar.tone,
    }
}

/// The row's name and state, as drawn: at rest the summary and the aside;
/// while it works, the name stays and the work's words are its state. A
/// row with no name while it works puts them where the name goes:
/// "installing · 30%", and a link row whose state the work says
/// ("connecting").
pub(crate) fn row_words(bar: &UiStackBar) -> (String, Option<String>) {
    match &bar.work {
        Some(work) if bar.summary.is_empty() => (work.words.clone(), None),
        Some(work) => (
            bar.summary.clone(),
            Some(work.words.clone()).filter(|words| !words.is_empty()),
        ),
        None => (bar.summary.clone(), bar.aside.clone()),
    }
}

/// How a row's state gives way to its name: a short word is shown whole or
/// not at all; a longer one is cut at its end, down to its first few
/// characters, and goes once fewer than those fit. Its class, and the
/// least of it worth showing.
fn aside_style(state: &str) -> (&'static str, String) {
    match state.chars().count() <= WHOLE_ASIDE_CHARS {
        true => (ASIDE_WHOLE_CLASS, String::new()),
        false => (ASIDE_CLASS, format!("--ux-aside-min: {ASIDE_MIN_CH}ch")),
    }
}

/// A state this short is shown whole or not at all ("crashing").
const WHOLE_ASIDE_CHARS: usize = 8;

/// The least of a longer state worth showing, its ellipsis included.
const ASIDE_MIN_CH: usize = 6;

/// The word of the row's own end section that opens its details
/// ("Review"), while the row carries no work.
fn opens_details(bar: &UiStackBar) -> Option<&'static str> {
    bar.opens_details.filter(|_| bar.work.is_none())
}

/// The row's line: the lead (the kind's icon; the spinner while it works,
/// the check once it went well), the kind, the name, the state, the mark,
/// and the section that opens its details.
fn bar_line(bar: &UiStackBar) -> Element {
    let toned = row_tone(bar) != UiStatusKind::Neutral;
    let working = matches!(
        bar.work.as_ref().map(|work| &work.state),
        Some(BarWorkState::Running)
    );
    let dim = if bar.stale { STALE_CLASS } else { "" };
    let icon_class = format!(
        "{} {dim}",
        if toned { ICON_TONED_CLASS } else { ICON_CLASS }
    );
    let kind_class = format!(
        "{} {dim}",
        if toned { KIND_TONED_CLASS } else { KIND_CLASS }
    );
    let state_class = format!(
        "{} {dim}",
        if toned {
            STATE_TONED_CLASS
        } else {
            STATE_CLASS
        }
    );
    let lead = match bar.work.as_ref().map(|work| &work.state) {
        Some(BarWorkState::Running) => rsx! {
            span { class: "{conic_spinner_class()} ux-board-spinner", aria_hidden: "true" }
        },
        Some(BarWorkState::Done) => rsx! {
            span { class: ICON_TONED_CLASS, aria_hidden: "true",
                StudioIcon { name: StudioIconName::StepComplete, size: 12 }
            }
        },
        _ => match action_icon_name(Some(bar.icon.as_str())) {
            Some(icon) => rsx! {
                span { class: "{icon_class}", aria_hidden: "true",
                    StudioIcon { name: icon, size: 12 }
                }
            },
            None => rsx! {},
        },
    };
    let (name, state) = row_words(bar);
    let name_class = format!(
        "{} {dim}",
        match (toned, working) {
            (true, _) => NAME_TONED_CLASS,
            (false, true) => NAME_WORKING_CLASS,
            (false, false) => NAME_CLASS,
        }
    );
    let aside = state.as_deref().map(aside_style);
    rsx! {
        {lead}
        if let Some(kind) = bar.kind {
            span { class: "{kind_class}", "data-bar-kind": "{kind}", "{kind}" }
        }
        // The name keeps its width; the state gives way first (the words'
        // wrap, style.css `.ux-board-bar-words`).
        span { class: WORDS_CLASS,
            span { class: "{name_class}", "{name}" }
            if let (Some(state), Some((aside_class, aside_min))) = (state, aside) {
                span {
                    class: "{state_class} {aside_class}",
                    style: aside_min,
                    "data-bar-state": "true",
                    "{state}"
                }
            }
        }
        if let Some(mark) = bar.mark.clone().filter(|_| bar.work.is_none()) {
            {bar_mark(mark)}
        }
        if let Some(word) = opens_details(bar) {
            span { class: OPENS_DETAILS_CLASS, "data-opens-details": "true", "{word}" }
        }
    }
}

/// The mark: its icon alone, its words the accessible name and the hover;
/// filled in the live tone while lit, struck through while it isn't.
fn bar_mark(mark: UiBarMark) -> Element {
    let icon = match (mark.icon.as_str(), mark.lit) {
        ("cloud", true) => Some(StudioIconName::CloudFilled),
        ("cloud", false) => Some(StudioIconName::CloudOff),
        (other, _) => action_icon_name(Some(other)),
    };
    let Some(icon) = icon else {
        return rsx! {};
    };
    rsx! {
        span {
            class: if mark.lit { MARK_LIT_CLASS } else { MARK_CLASS },
            role: "img",
            aria_label: "{mark.label}",
            title: "{mark.label}",
            "data-bar-mark": "{mark.icon}",
            "data-lit": if mark.lit { "true" } else { "false" },
            StudioIcon { name: icon, size: 12 }
        }
    }
}

/// The row's hover title: its whole line, which the row may cut, and its
/// mark's words.
fn bar_title(bar: &UiStackBar) -> String {
    let (name, state) = row_words(bar);
    let mut line: Vec<&str> = bar.kind.into_iter().collect();
    line.push(&name);
    let mut title = line.join(" ");
    if let Some(state) = &state {
        title.push_str(" · ");
        title.push_str(state);
    }
    match (&bar.work, &bar.mark) {
        (None, Some(mark)) => format!("{title} · {}", mark.label),
        _ => title,
    }
}

/// The row's name, for its details' accessible name.
fn layer_name(layer: lpa_studio_core::BarLayer) -> &'static str {
    match layer {
        lpa_studio_core::BarLayer::Project => "Project",
        lpa_studio_core::BarLayer::Usb => "USB",
        lpa_studio_core::BarLayer::Wifi => "Wi\u{2011}Fi",
        lpa_studio_core::BarLayer::Bluetooth => "Bluetooth",
        lpa_studio_core::BarLayer::Cloud => "lightplayer.app",
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

/// The fixed row: 28 px with its top hairline (the rows are full-bleed rows
/// of one card, never boxes), the last one rounded with the card.
const BAR_BASE_CLASS: &str = "ux-board-bar tw:relative tw:flex tw:h-7 tw:min-w-0 tw:items-stretch tw:border-0 tw:border-t tw:border-solid tw:text-[11.5px] tw:last:rounded-b-[7px]";

/// The trigger's slot: the rest of the row, its popover wrapper stretched to
/// it (the wrapper is an inline grid that centres its button). Positioned:
/// a pick the details hand back opens over it.
const TRIGGER_SLOT_CLASS: &str = "tw:relative tw:grid tw:min-w-0 tw:flex-1 tw:[&>span]:h-full tw:[&>span]:w-full tw:[&>span]:place-items-stretch";

/// The trigger: the row's icon, kind, name and state as one button, the
/// row's own ink. Tailwind preflight is not loaded, so the UA chrome is
/// reset.
const TRIGGER_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:px-2.5 tw:text-left tw:text-[11.5px] tw:text-inherit tw:transition-colors tw:hover:bg-white/5 ux-focus-ring";

/// …while its details are open.
const TRIGGER_OPEN_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-white/5 tw:px-2.5 tw:text-left tw:text-[11.5px] tw:text-inherit ux-focus-ring";

const ICON_CLASS: &str = "tw:inline-flex tw:flex-none tw:text-subtle-foreground";
const ICON_TONED_CLASS: &str = "tw:inline-flex tw:flex-none";
/// The kind: a fixed column (56 px) so the names line up down the card,
/// muted — it teaches, it isn't news.
const KIND_CLASS: &str =
    "tw:w-14 tw:flex-none tw:font-medium tw:text-[color:var(--studio-color-text-faint)]";
/// …in a toned row: the row's own colour, quieter.
const KIND_TONED_CLASS: &str = "tw:w-14 tw:flex-none tw:font-medium tw:opacity-70";
/// The name and the state: one 28 px line that the state wraps off when
/// it can't fit beside the whole name (style.css).
const WORDS_CLASS: &str = "ux-board-bar-words";
/// The name: its whole width before the state has any, cut only when it
/// alone is wider than the row.
const NAME_CLASS: &str = "ux-board-bar-name tw:text-foreground";
const NAME_TONED_CLASS: &str = "ux-board-bar-name tw:font-bold";
/// The name while the row works: strong, the row neutral.
const NAME_WORKING_CLASS: &str = "ux-board-bar-name tw:font-bold tw:text-strong-foreground";
const STATE_CLASS: &str = "tw:text-subtle-foreground";
/// …in a toned row: the row's own ink.
const STATE_TONED_CLASS: &str = "";
/// A longer state: cut at its end, down to `--ux-aside-min`.
const ASIDE_CLASS: &str = "ux-board-bar-aside";
/// A short state: whole, or wrapped off.
const ASIDE_WHOLE_CLASS: &str = "ux-board-bar-aside-whole";
/// What a stale row's words wear: last known, dimmed.
const STALE_CLASS: &str = "tw:opacity-45";
const CHEVRON_CLASS: &str = "tw:inline-flex tw:flex-none tw:text-dim-foreground";
/// The row's own end section that opens its details ("Review"): drawn as a
/// flush section, inside the trigger because opening the details is all it
/// does.
const OPENS_DETAILS_CLASS: &str = "tw:-mr-2.5 tw:ml-1 tw:inline-flex tw:h-full tw:flex-none tw:items-center tw:whitespace-nowrap tw:border-0 tw:border-l tw:border-solid tw:border-[color:var(--ux-row-flush-edge,var(--studio-color-border))] tw:bg-white/[0.035] tw:px-[11px] tw:text-[11px] tw:font-extrabold tw:text-strong-foreground";
/// A mark not holding: struck through, dim.
const MARK_CLASS: &str = "tw:inline-flex tw:flex-none tw:items-center tw:text-dim-foreground";
/// A lit mark: filled, the live tone (the board reachable through
/// lightplayer.app).
const MARK_LIT_CLASS: &str =
    "tw:inline-flex tw:flex-none tw:items-center tw:text-status-live-foreground";

#[cfg(test)]
mod tests {
    use super::*;

    /// The cloud mark is the icon alone: no words on the row, its label the
    /// accessible name and the hover, lit in the live tone when the board is
    /// reachable through lightplayer.app.
    #[test]
    fn a_mark_is_an_icon_alone_with_its_words_as_its_name() {
        let bar = link_row(Some(UiBarMark {
            icon: "cloud".to_string(),
            label: "Also reachable through lightplayer.app".to_string(),
            lit: true,
        }));
        let html = dioxus_ssr::render_element(bar_line(&bar));
        assert!(html.contains("data-bar-mark=\"cloud\""), "{html}");
        assert!(html.contains("data-lit=\"true\""), "{html}");
        assert!(
            html.contains("aria-label=\"Also reachable through lightplayer.app\""),
            "{html}"
        );
        assert!(!html.contains(">also cloud<"), "no words: {html}");
        assert!(MARK_LIT_CLASS.contains("status-live"));
        assert_eq!(
            bar_title(&bar),
            "USB connected · Also reachable through lightplayer.app"
        );
    }

    /// P14: lit and not lit differ in shape, not only in colour — filled
    /// against struck through.
    #[test]
    fn a_lit_cloud_is_filled_and_an_unreachable_one_struck_through() {
        let lit = dioxus_ssr::render_element(bar_mark(UiBarMark {
            icon: "cloud".to_string(),
            label: "lit".to_string(),
            lit: true,
        }));
        let off = dioxus_ssr::render_element(bar_mark(UiBarMark {
            icon: "cloud".to_string(),
            label: "off".to_string(),
            lit: false,
        }));
        assert!(lit.contains("fill=\"currentColor\""), "{lit}");
        assert!(!off.contains("fill=\"currentColor\""), "{off}");
        assert_ne!(
            lit.matches("<path").count(),
            off.matches("<path").count(),
            "struck through draws the strike: {off}"
        );
    }

    /// The row's grammar: the kind's word in its own column after the icon,
    /// then the name, then the state; a link row's kind is the link's name.
    #[test]
    fn a_row_reads_icon_kind_name_state() {
        let mut row = lpa_studio_core::UiStackBar::new(
            lpa_studio_core::BarLayer::Project,
            "project",
            "studio",
        );
        row.aside = Some("out of date".to_string());
        let html = dioxus_ssr::render_element(bar_line(&row));
        let kind = html.find("data-bar-kind=\"Project\"").expect("the kind");
        let name = html.find(">studio<").expect("the name");
        let state = html.find(">out of date<").expect("the state");
        assert!(kind < name && name < state, "{html}");
        assert_eq!(bar_title(&row), "Project studio · out of date");

        let link = link_row(None);
        let html = dioxus_ssr::render_element(bar_line(&link));
        let kind = html.find("data-bar-kind=\"USB\"").expect("the link's name");
        let name = html.find(">connected<").expect("how it stands");
        assert!(kind < name, "{html}");
    }

    /// The name is the row's value and keeps its width: it and the state
    /// share one wrapping line, the state after it. A short state is whole
    /// or wrapped off; a longer one is cut down to its first few characters
    /// before it goes. The name is never the one that gives way first.
    #[test]
    fn the_state_gives_way_before_the_name() {
        let mut row = lpa_studio_core::UiStackBar::new(
            lpa_studio_core::BarLayer::Firmware,
            "firmware",
            "v2026.10.03-1",
        );
        row.aside = Some("crashing".to_string());
        let html = dioxus_ssr::render_element(bar_line(&row));
        let words = html.find(WORDS_CLASS).expect("the words' line");
        let name = html.find(">v2026.10.03-1<").expect("the name");
        let state = html.find(">crashing<").expect("the state");
        assert!(words < name && name < state, "{html}");
        assert!(
            html.contains(ASIDE_WHOLE_CLASS),
            "a short state is whole: {html}"
        );

        row.aside = Some("older LightPlayer".to_string());
        let html = dioxus_ssr::render_element(bar_line(&row));
        assert!(html.contains(&format!("{ASIDE_CLASS}\"")), "{html}");
        assert!(html.contains("--ux-aside-min: 6ch"), "{html}");
        for class in [NAME_CLASS, NAME_TONED_CLASS, NAME_WORKING_CLASS] {
            assert!(
                !class.contains("tw:flex-1"),
                "the CSS sizes the name: {class}"
            );
        }
    }

    /// While a row works its name stays and the work's words are its
    /// state; a row with no name while it works says them in its place.
    #[test]
    fn work_keeps_the_name_and_says_its_state() {
        let mut row = lpa_studio_core::UiStackBar::new(
            lpa_studio_core::BarLayer::Project,
            "project",
            "Playful Choker",
        );
        row.work = Some(running("sending · 40%"));
        assert_eq!(
            row_words(&row),
            (
                "Playful Choker".to_string(),
                Some("sending · 40%".to_string())
            )
        );
        let mut link = link_row(None);
        link.summary = String::new();
        link.work = Some(running("connecting"));
        assert_eq!(row_words(&link), ("connecting".to_string(), None));
        let mut nameless =
            lpa_studio_core::UiStackBar::new(lpa_studio_core::BarLayer::Firmware, "firmware", "");
        nameless.work = Some(running("installing · 30%"));
        assert_eq!(row_words(&nameless), ("installing · 30%".to_string(), None));
    }

    /// A stale row is dimmed and says the same words.
    #[test]
    fn a_stale_row_is_dimmed_with_the_same_words() {
        let mut row = lpa_studio_core::UiStackBar::new(
            lpa_studio_core::BarLayer::Firmware,
            "firmware",
            "v2026.10.08-9",
        );
        let fresh = dioxus_ssr::render_element(bar_line(&row));
        row.stale = true;
        let stale = dioxus_ssr::render_element(bar_line(&row));
        assert!(!fresh.contains(STALE_CLASS), "{fresh}");
        assert!(stale.contains(STALE_CLASS), "{stale}");
        assert!(stale.contains(">v2026.10.08-9<"), "{stale}");
    }

    const KINDS: [UiStatusKind; 7] = [
        UiStatusKind::Neutral,
        UiStatusKind::Working,
        UiStatusKind::Good,
        UiStatusKind::Live,
        UiStatusKind::Warning,
        UiStatusKind::Attention,
        UiStatusKind::Error,
    ];

    /// Every tone maps to a tint in its own family — Neutral to none.
    #[test]
    fn every_tone_tints_in_its_family() {
        for kind in KINDS {
            let class = bar_tone_class(kind);
            match kind {
                UiStatusKind::Neutral => {
                    assert!(!class.contains("tw:bg-"), "{class}");
                    assert!(!class.contains("status"), "{class}");
                }
                _ => {
                    let family = format!("{kind:?}").to_lowercase();
                    for part in ["border", "bg", "foreground"] {
                        let token = format!("status-{family}-{part}");
                        assert!(class.contains(&token), "{token} missing: {class}");
                    }
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

    /// A toned row keeps its kind's icon: the state's word says what, the
    /// tone how bad (P14).
    #[test]
    fn a_toned_row_keeps_its_kinds_icon() {
        let mut row = lpa_studio_core::UiStackBar::new(
            lpa_studio_core::BarLayer::Firmware,
            "firmware",
            "v2026.10.08-9",
        );
        row.aside = Some("crashing".to_string());
        row.tone = UiStatusKind::Error;
        let toned = dioxus_ssr::render_element(bar_line(&row));
        row.tone = UiStatusKind::Neutral;
        row.aside = None;
        let quiet = dioxus_ssr::render_element(bar_line(&row));
        let svg = |html: &str| html[html.find("<svg").expect("an icon")..].to_string();
        assert_eq!(
            svg(&toned).split('>').nth(1),
            svg(&quiet).split('>').nth(1),
            "the same glyph"
        );
    }

    /// A toned row keeps its height: the row is the same fixed 28 px in
    /// every tone, one line, never wrapping.
    #[test]
    fn a_tinted_bar_keeps_its_height() {
        for kind in KINDS {
            let class = bar_class(kind);
            assert!(class.contains("tw:h-7"), "{class}");
            assert!(!class.contains("min-h"), "{class}");
            assert!(!class.contains("tw:py-"), "{class}");
        }
        for class in [NAME_CLASS, NAME_TONED_CLASS, NAME_WORKING_CLASS] {
            assert!(class.contains("ux-board-bar-name"), "{class}");
        }
        assert!(TRIGGER_CLASS.contains("tw:h-full"));
        assert!(TRIGGER_OPEN_CLASS.contains("tw:h-full"));
        assert!(KIND_CLASS.contains("tw:w-14") && KIND_TONED_CLASS.contains("tw:w-14"));
    }

    /// The rows' hairlines: every row carries the full-bleed top border,
    /// and nothing inside a row draws a frame of its own.
    #[test]
    fn every_bar_carries_the_full_bleed_hairline() {
        assert!(BAR_BASE_CLASS.contains("tw:border-t tw:border-solid"));
        assert!(
            !BAR_BASE_CLASS.contains("tw:px-"),
            "the row pads its pieces, not itself"
        );
        for class in [
            TRIGGER_CLASS,
            ICON_CLASS,
            KIND_CLASS,
            NAME_CLASS,
            STATE_CLASS,
        ] {
            assert!(!class.contains("rounded"), "{class}");
            assert!(!class.contains("tw:border "), "{class}");
        }
    }

    fn link_row(mark: Option<UiBarMark>) -> UiStackBar {
        UiStackBar {
            mark,
            ..UiStackBar::new(lpa_studio_core::BarLayer::Usb, "usb", "connected")
        }
    }

    fn running(words: &str) -> lpa_studio_core::UiBarWork {
        lpa_studio_core::UiBarWork {
            words: words.to_string(),
            detail: None,
            percent: Some(40),
            state: BarWorkState::Running,
            cancel: None,
            other_device: false,
        }
    }
}

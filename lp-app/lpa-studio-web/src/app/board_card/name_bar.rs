//! [`NameBar`]: the board's name (one line, cut, its whole on hover), its
//! place under it when core gives one (the bar keeps its 50 px either way),
//! and the one primary as a flush section at the bar's end — icon before
//! word, the spectrum ring on hover (`docs/style/ui.md` "The board card").
//!
//! The bar carries `data-name-bar`, the walks' hook for the primary (an
//! empty board's Add a project is pressed there).
//!
//! The name is renamed where it is ([`RenamableName`]): while core offers
//! `rename` ([`UiNameBar::rename`]) the name is a button with a faint pencil
//! after it, always shown, brighter (and the name underlined) on hover.
//! Pressing it — a click, or Enter or Space on it — puts a field in the
//! name's place at the bar's height, its text selected; Enter or clicking
//! away presses the offer with the new name, and Esc puts the name back.
//! A blank or unchanged name presses nothing. No Save button: the field is
//! the page's edit state, and the press is core's.
//!
//! The name keeps about twelve characters: on a card too narrow for that
//! beside the whole primary, the primary drops to its verb alone ("Update"
//! for "Update firmware"; core's [`UiCardAction::verb_alone`]), since the
//! surface's sentence already names the firmware ([`primary_need`],
//! style.css `.ux-label-tail`).
//!
//! While the board is being reached or identified the primary is Studio's
//! conic spinner alone in the same section ([`UiPrimary::Working`]), its
//! label the accessible name: the Link row (or the Hardware row) says how.
//! When nothing can be pressed there is no section at all — never a greyed
//! verb, and never a row's words said again here (P2, P7). The spinner
//! presses nothing, so it is not an action.

use dioxus::prelude::*;
use lpa_studio_core::{OfferPath, UiAction, UiCardAction, UiNameBar, UiPrimary};

use super::card_action::{CardAction, CardActionLook, narrow_tail};
use crate::app::agent::AgentMark;
use crate::app::home::device_access_panel::select_focused_input;
use crate::app::home::device_rename_section::rename_press;
use crate::base::{StudioIcon, StudioIconName, conic_spinner_class};
use crate::core::use_offer_at;

/// The name bar. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NameBar(bar: UiNameBar, on_action: EventHandler<UiAction>) -> Element {
    // The card width the whole primary needs beside the name's reserve:
    // narrower, its word drops to the verb alone (style.css
    // `.ux-label-tail`).
    let need = match &bar.primary {
        Some(UiPrimary::Offer(action)) => primary_need(&bar.title, action),
        _ => None,
    }
    .map(|px| format!("--ux-primary-need: {px}px"))
    .unwrap_or_default();
    let primary = match bar.primary {
        Some(UiPrimary::Offer(action)) => rsx! {
            CardAction { action, look: CardActionLook::Primary, on_action }
        },
        Some(UiPrimary::Working { label }) => rsx! {
            WorkingPrimary { label }
        },
        None => rsx! {},
    };
    rsx! {
        div { class: NAME_BAR_CLASS, "data-name-bar": "true", style: need,
            div { class: NAME_BLOCK_CLASS,
                if let Some(rename) = bar.rename.clone() {
                    RenamableName { title: bar.title.clone(), rename, on_action }
                } else {
                    h3 { class: TITLE_CLASS, title: "{bar.title}", "{bar.title}" }
                }
                if let Some(place) = bar.place {
                    p { class: PLACE_CLASS, title: "{place}", "{place}" }
                }
            }
            {primary}
        }
    }
}

/// The name, renamed where it is: a button with a faint pencil, and while
/// it is pressed a field in its place. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RenamableName(title: String, rename: OfferPath, on_action: EventHandler<UiAction>) -> Element {
    let offer = use_offer_at(rename.clone())();
    let mut editing = use_signal(|| false);
    let mut value = use_signal(|| title.clone());
    // Esc puts the name back; the blur that follows must not press.
    let mut cancelled = use_signal(|| false);
    let Some(offer) = offer else {
        return rsx! {
            h3 { class: TITLE_CLASS, title: "{title}", "{title}" }
        };
    };
    let was = title.clone();
    let commit = move |end: RenameEnd| {
        editing.set(false);
        let end = match std::mem::take(&mut *cancelled.write()) {
            true => RenameEnd::Escape,
            false => end,
        };
        if let Some(name) = renamed(end, &value.read(), &was)
            && let Some(action) = rename_press(&offer, &name)
        {
            on_action.call(action);
        }
    };
    if editing() {
        let reset = title.clone();
        let mut enter = commit.clone();
        let mut blur = commit;
        return rsx! {
            AgentMark { path: rename.clone(),
                input {
                    class: RENAME_FIELD_CLASS,
                    "data-rename-field": "true",
                    aria_label: "Board name",
                    value: "{value}",
                    onmounted: move |event| async move {
                        let _ = event.set_focus(true).await;
                        select_focused_input();
                    },
                    oninput: move |event| value.set(event.value()),
                    onkeydown: move |event| match event.key() {
                        Key::Enter => {
                            event.prevent_default();
                            enter(RenameEnd::Enter);
                        }
                        Key::Escape => {
                            event.prevent_default();
                            cancelled.set(true);
                            value.set(reset.clone());
                            editing.set(false);
                        }
                        _ => {}
                    },
                    onblur: move |_| {
                        if editing() {
                            blur(RenameEnd::Blur);
                        }
                    },
                }
            }
        };
    }
    let start = title.clone();
    rsx! {
        AgentMark { path: rename,
            button {
                class: NAME_BUTTON_CLASS,
                r#type: "button",
                title: "Rename {title}",
                aria_label: "Rename {title}",
                "data-rename": "true",
                onclick: move |_| {
                    value.set(start.clone());
                    cancelled.set(false);
                    editing.set(true);
                },
                span { class: "{TITLE_CLASS} ux-board-name-text", "{title}" }
                span { class: PENCIL_CLASS, aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Edited, size: 12 }
                }
            }
        }
    }
}

/// How a rename ends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenameEnd {
    /// Enter: keep it.
    Enter,
    /// Clicking away: keep it.
    Blur,
    /// Esc: put the name back.
    Escape,
}

/// The name a rename ending in `end` presses, from what was `typed` over
/// the name that `was`: kept on Enter or a click away, trimmed; nothing on
/// Esc, and nothing for a blank or unchanged name.
fn renamed(end: RenameEnd, typed: &str, was: &str) -> Option<String> {
    let name = typed.trim();
    match end {
        RenameEnd::Escape => None,
        RenameEnd::Enter | RenameEnd::Blur => {
            (!name.is_empty() && name != was).then(|| name.to_string())
        }
    }
}

/// The spinner alone, in the primary's section: the board is being
/// reached or identified.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WorkingPrimary(label: String) -> Element {
    rsx! {
        div {
            class: WORKING_CLASS,
            role: "status",
            aria_label: "{label}",
            title: "{label}",
            "data-primary-working": "true",
            span { class: "{conic_spinner_class()} ux-board-spinner", aria_hidden: "true" }
        }
    }
}

/// The card width, in px, the whole primary needs beside the board's name
/// at its reserve — about twelve characters, or the whole name when it is
/// shorter — or `None` for a primary with nothing to drop. An estimate
/// from the words' letters at the bar's type sizes: below it the name
/// would be cut short of its reserve, so the word drops to its verb alone
/// (the surface's sentence names the object).
fn primary_need(title: &str, action: &UiCardAction) -> Option<usize> {
    narrow_tail(action, CardActionLook::Primary)?;
    let name = text_tenths(title).min(NAME_RESERVE_CHARS * GLYPH_TENTHS) * NAME_SCALE_PERCENT / 100;
    let icon = match action.icon {
        Some(_) => PRIMARY_ICON_TENTHS,
        None => 0,
    };
    let word = text_tenths(&action.word);
    let need = BAR_EDGES_TENTHS + name + PRIMARY_PADDING_TENTHS + icon + word + MARGIN_TENTHS;
    Some(need.div_ceil(10))
}

/// About how wide `text` is at the primary's 12.5 px extrabold, in tenths
/// of a px: narrow letters, wide ones, capitals and the rest.
fn text_tenths(text: &str) -> usize {
    text.chars()
        .map(|glyph| match glyph {
            'i' | 'j' | 'l' | 'r' | 't' | 'f' | 'I' | ' ' | '.' | ',' | '-' | '\'' => 40,
            'm' | 'w' | 'M' | 'W' => 105,
            'A'..='Z' => 85,
            _ => GLYPH_TENTHS,
        })
        .sum()
}

/// The name's reserve: about twelve characters.
const NAME_RESERVE_CHARS: usize = 12;
/// An ordinary letter at 12.5 px extrabold, in tenths of a px.
const GLYPH_TENTHS: usize = 75;
/// The name's 13.5 px bold against the primary's type.
const NAME_SCALE_PERCENT: usize = 106;
/// The bar's left padding (11 px) and its gap (8 px).
const BAR_EDGES_TENTHS: usize = 190;
/// The primary's padding (16 px each side) and its leading hairline.
const PRIMARY_PADDING_TENTHS: usize = 330;
/// The primary's icon (12 px) and its gap (6 px).
const PRIMARY_ICON_TENTHS: usize = 180;
/// A few px over: a guess at the letters' widths must not cost the name its
/// last letter (measured on the stories: within 3 px).
const MARGIN_TENTHS: usize = 40;

/// The bar: 50 px with its top hairline, flush to the card's right edge so
/// the primary's section reaches it. Nothing here clips: the primary's
/// hover glow reaches past its section.
const NAME_BAR_CLASS: &str = "tw:flex tw:h-[50px] tw:min-w-0 tw:items-stretch tw:gap-2 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:pl-[11px]";

/// The name and its place, centred in the bar.
const NAME_BLOCK_CLASS: &str = "tw:grid tw:min-w-0 tw:flex-1 tw:content-center tw:gap-px";

/// The name: one line, cut.
const TITLE_CLASS: &str = "tw:m-0 tw:min-w-0 tw:truncate tw:text-[13.5px] tw:font-bold tw:leading-tight tw:text-strong-foreground";

/// The name as a button: the title's look, then the pencil; the cut stays
/// on the name. Hover brightens the pencil and underlines the name
/// (style.css `.ux-board-name`).
const NAME_BUTTON_CLASS: &str = "ux-board-name tw:flex tw:min-w-0 tw:max-w-full tw:cursor-text tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:p-0 tw:text-left tw:text-inherit ux-focus-ring tw:rounded-sm";

/// The pencil after the name: faint, always shown.
const PENCIL_CLASS: &str = "ux-board-name-pencil tw:inline-flex tw:flex-none tw:text-[color:var(--studio-color-text-faint)]";

/// The field in the name's place: the name's type, the bar's height less
/// its padding, Studio's input ground.
const RENAME_FIELD_CLASS: &str = "tw:h-[30px] tw:w-full tw:min-w-0 tw:rounded-[5px] tw:border tw:border-solid tw:border-border-strong tw:bg-terminal tw:px-2 tw:text-[13.5px] tw:font-bold tw:text-strong-foreground ux-focus-ring";

/// The place under the name.
const PLACE_CLASS: &str =
    "tw:m-0 tw:min-w-0 tw:truncate tw:text-[11.5px] tw:leading-tight tw:text-subtle-foreground";

/// The spinner's section: the row's height and the primary's own leading
/// hairline, flush to the card's edge, quiet (no wash, no ring: nothing to
/// press).
const WORKING_CLASS: &str = "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:self-stretch tw:border-0 tw:border-l tw:border-solid tw:border-border tw:px-[18px]";

#[cfg(test)]
mod tests {
    use super::*;

    /// The name bar is one fixed 50 px row whose name never wraps.
    #[test]
    fn the_name_bar_is_fixed_and_its_name_one_line() {
        assert!(NAME_BAR_CLASS.contains("tw:h-[50px]"));
        assert!(!NAME_BAR_CLASS.contains("min-h"));
        assert!(TITLE_CLASS.contains("tw:truncate"));
        assert!(PLACE_CLASS.contains("tw:truncate"));
    }

    /// The primary's glow is a bloom past its section, so nothing around it
    /// clips; and the primary's look is the ringed flush section.
    #[test]
    fn nothing_around_the_primary_verb_clips_its_glow() {
        use crate::core::ActionButtonVariant;
        use crate::core::action::action_variant_class;
        for class in [NAME_BAR_CLASS, NAME_BLOCK_CLASS, WORKING_CLASS] {
            assert!(!class.contains("overflow"), "{class}");
        }
        let primary = action_variant_class(ActionButtonVariant::RowPrimary, false);
        assert!(primary.contains("ux-ir-ring"), "{primary}");
        assert!(!primary.contains("ux-spectrum-cta"), "{primary}");
        assert!(WORKING_CLASS.contains("tw:self-stretch"));
    }

    /// Enter and a click away keep the typed name (trimmed); Esc puts the
    /// name back; a blank or unchanged name presses nothing.
    #[test]
    fn enter_and_blur_keep_the_name_and_esc_puts_it_back() {
        for end in [RenameEnd::Enter, RenameEnd::Blur] {
            assert_eq!(renamed(end, "  Eaves  ", "Porch").as_deref(), Some("Eaves"));
            assert_eq!(renamed(end, "   ", "Porch"), None, "blank");
            assert_eq!(renamed(end, "Porch", "Porch"), None, "unchanged");
        }
        assert_eq!(renamed(RenameEnd::Escape, "Eaves", "Porch"), None);
    }

    /// While rename is offered the name is a button with the pencil after
    /// it, marked with the offer's path; without it, the plain name.
    #[test]
    fn the_name_is_pressable_with_a_pencil_while_rename_is_offered() {
        let (card, tree) = super::super::card_test_fixtures::card_and_tree(
            &super::super::card_test_fixtures::porch_view(),
        );
        let rename = card.name_bar.rename.clone().expect("rename is offered");
        assert!(rename.to_string().ends_with("/rename"));
        let html = super::super::card_test_fixtures::render_card(card.clone(), tree.clone(), None);
        assert!(html.contains("data-rename=\"true\""), "{html}");
        assert!(html.contains("ux-board-name-pencil"), "{html}");
        assert!(html.contains("aria-label=\"Rename Porch\""), "{html}");
        let mut plain = card;
        plain.name_bar.rename = None;
        let html = super::super::card_test_fixtures::render_card(plain, tree, None);
        assert!(!html.contains("data-rename"), "{html}");
    }

    /// Connecting or identifying: the spinner alone, named for a screen
    /// reader, no words and no button.
    #[test]
    fn a_working_primary_is_the_spinner_alone() {
        let html = super::super::card_test_fixtures::render(working, ());
        assert!(html.contains("data-primary-working"), "{html}");
        assert!(html.contains("aria-label=\"Connecting\""), "{html}");
        assert!(!html.contains("<button"), "nothing to press: {html}");
    }

    /// The name keeps about twelve characters: the whole primary needs its
    /// width beside them, a longer word more, a shorter name less; a
    /// primary that is its verb alone has nothing to drop.
    #[test]
    fn the_primary_names_the_width_it_needs_beside_the_name() {
        let update = primary("Update firmware", Some("Update"));
        let need = primary_need("XIAO ESP32-C6", &update).expect("a tail to drop");
        assert!((240..300).contains(&need), "{need}");
        let reinstall = primary("Reinstall firmware", Some("Reinstall"));
        assert!(primary_need("XIAO ESP32-C6", &reinstall).unwrap() > need);
        assert!(primary_need("Lamp", &update).unwrap() < need);
        assert_eq!(
            primary_need("A very long board name", &update),
            Some(need),
            "the reserve, not the whole name"
        );
        assert_eq!(
            primary_need("XIAO ESP32-C6", &primary("Connect", None)),
            None
        );
    }

    /// The primary's tail is its own span, so a narrow card can drop it;
    /// the button's text stays whole.
    #[test]
    fn the_primary_draws_its_tail_apart() {
        let html = super::super::card_test_fixtures::render(tail_label, ());
        assert!(
            html.contains("Update<span class=\"ux-label-tail\"> firmware</span>"),
            "{html}"
        );
    }

    fn tail_label() -> Element {
        crate::core::action::action_button::label_text("Update firmware", Some(" firmware"))
    }

    fn primary(word: &str, verb: Option<&str>) -> UiCardAction {
        UiCardAction {
            offer: lpa_studio_core::OfferPath::parse("devices/new-1/update-firmware")
                .expect("a path"),
            word: word.to_string(),
            icon: Some("firmware".to_string()),
            draw: lpa_studio_core::UiActionDraw::Press,
            args: lpa_studio_core::OfferArgs::new(),
            refused: None,
            verb_alone: verb.map(str::to_string),
        }
    }

    fn working() -> Element {
        rsx! {
            WorkingPrimary { label: "Connecting".to_string() }
        }
    }
}

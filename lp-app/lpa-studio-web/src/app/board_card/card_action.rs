//! [`CardAction`]: one button on the board card — a name bar's primary, a
//! bar's action, a verb in a bar's details — drawn from the offer it
//! presses ([`UiCardAction`]), in the card's word and icon.
//!
//! Core decides the offer, the word, the icon, the preset values and which
//! picker draws it ([`UiActionDraw`]); this piece only picks the look
//! ([`CardActionLook`]). It never builds an action: a press is the offer's
//! own ([`UiOffer::press`]), relabelled with the card's word, so the app
//! agent pressing the same path does exactly what the button does. An
//! offer the tree no longer holds draws nothing.
//!
//! Every action sits inside an [`AgentMark`], so `data-offer-path` is on
//! every button the card draws (the walk hook, DC24).
//!
//! [`OfferAction`] is the same for a bare offer path the card names without
//! words of its own (a running work's Cancel, a layout question's
//! buttons): the offer's own action, as published.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceId, OfferArgs, OfferPath, UiAction, UiActionDraw, UiCardAction, UiExampleCard, UiOffer,
    UiPackageCard,
};

use crate::app::agent::AgentMark;
use crate::app::home::device_pick_popover::{
    BoardPickMode, BoardPickPopover, ChipSource, ProjectPickMode, ProjectPickPopover, VerbTrigger,
};
use crate::base::action_icon_name;
use crate::core::action::action_variant_class;
use crate::core::{
    ActionButton, ActionButtonVariant, OfferParamsForm, OfferPressButton, pressed_or_refused,
    use_offer_at,
};

/// How a card action looks where it sits. Only the look: the word, the
/// icon and the offer are core's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardActionLook {
    /// The name bar's one primary: a flush section, ringed on hover.
    Primary,
    /// A bar's action: a flush section at the bar's end.
    BarEnd,
    /// A verb in a details card: a menu row.
    MenuItem,
}

impl CardActionLook {
    /// The [`ActionButton`] variant this look draws a press with.
    pub fn variant(self) -> ActionButtonVariant {
        match self {
            CardActionLook::Primary => ActionButtonVariant::RowPrimary,
            CardActionLook::BarEnd => ActionButtonVariant::RowEnd,
            CardActionLook::MenuItem => ActionButtonVariant::MenuItem,
        }
    }

    /// A flush section of a fixed row (it fills the row's height).
    fn flush(self) -> bool {
        self.variant().is_row_section()
    }
}

/// What every action and panel under one card shares: the page's lists
/// (the project pick reads them), the board's name (a new project's default
/// name), its roster handle (the panels that hand core a file, or flip the
/// Bluetooth switch, name the board by it), and a story's previews.
/// Provided by [`super::BoardCard`]; outside one an action has empty lists
/// and nothing armed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CardScope {
    pub projects: Vec<UiPackageCard>,
    pub examples: Vec<UiExampleCard>,
    pub board_title: String,
    pub device: Option<DeviceId>,
    /// Stories: the action at this path renders already armed.
    pub armed: Option<OfferPath>,
    /// Stories: panels mounted in a state a capture cannot click to.
    pub previews: CardPreviews,
}

/// Stories only: panels mounted in a state a capture cannot click to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardPreviews {
    /// The access panel's keys list, open.
    pub access_keys_open: bool,
    /// The other-version form with values picked (and armed).
    pub other_version: Option<super::other_version_form::OfferPickerPreview>,
}

/// Provide `scope` to every action below the caller; readers re-render
/// only when it changes.
pub(crate) fn use_provide_card_scope(scope: CardScope) {
    let mut provided = use_context_provider(|| Signal::new(scope.clone()));
    if *provided.peek() != scope {
        provided.set(scope);
    }
}

/// The card's shared facts, or an empty scope outside a card.
pub(crate) fn use_card_scope() -> CardScope {
    let fallback = use_signal(CardScope::default);
    let scope = use_hook(try_consume_context::<Signal<CardScope>>).unwrap_or(fallback);
    scope.read().clone()
}

/// One card action. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CardAction(
    action: UiCardAction,
    look: CardActionLook,
    on_action: EventHandler<UiAction>,
) -> Element {
    let scope = use_card_scope();
    let Some(offer) = use_offer_at(action.offer.clone())() else {
        return rsx! {};
    };
    let armed = scope.armed.as_ref() == Some(&action.offer);
    let path = action.offer.clone();
    // A refused offer is the press, disabled with its reason, whichever
    // picker would have drawn it: a picker with nothing to pick says
    // nothing a disabled button does not.
    let draw = match action.refused {
        Some(_) => UiActionDraw::Press,
        None => action.draw.clone(),
    };
    let body = match draw {
        UiActionDraw::Press | UiActionDraw::Sheet => rsx! {
            ActionButton {
                action: card_press(&offer, &action),
                running: false,
                variant: look.variant(),
                armed_preview: armed,
                on_action,
            }
        },
        UiActionDraw::ProjectPick { board_id } => rsx! {
            PickSlot { look,
                ProjectPickPopover {
                    offer,
                    board_id,
                    board_title: scope.board_title.clone(),
                    projects: scope.projects.clone(),
                    examples: scope.examples.clone(),
                    mode: ProjectPickMode::Verb,
                    initial_args: action.args.clone(),
                    verb_trigger: verb_trigger(&action, look),
                    on_action,
                }
            }
        },
        UiActionDraw::BoardPick {
            chip,
            chip_from_banner,
        } => rsx! {
            PickSlot { look,
                BoardPickPopover {
                    offer,
                    chip: chip.map(|chip| (chip, chip_source(chip_from_banner))),
                    mode: BoardPickMode::Verb,
                    initial_args: action.args.clone(),
                    verb_trigger: verb_trigger(&action, look),
                    on_action,
                }
            }
        },
        UiActionDraw::Choice => rsx! {
            ChoiceAction {
                offer,
                args: action.args.clone(),
                look,
                armed,
                on_action,
            }
        },
    };
    rsx! {
        AgentMark { path, {body} }
    }
}

/// A bare offer path, drawn as the offer publishes it (its own word and
/// icon) — a running work's Cancel, a layout question's buttons. Nothing
/// when the tree no longer offers it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OfferAction(
    path: OfferPath,
    /// How the button looks where it sits.
    variant: ActionButtonVariant,
    /// The surface around the button IS the question its press answers (a
    /// layout question's Continue): a Lasting press acts at once.
    #[props(default)]
    asked_by_surface: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let scope = use_card_scope();
    let Some(offer) = use_offer_at(path.clone())() else {
        return rsx! {};
    };
    rsx! {
        AgentMark { path: path.clone(),
            ActionButton {
                action: offer.action,
                running: false,
                variant,
                armed_preview: scope.armed.as_ref() == Some(&path),
                asked_by_surface,
                on_action,
            }
        }
    }
}

/// The offer's params drawn inline ([`UiActionDraw::Choice`]), and its press
/// with what they hold — no popover: a details card is already one.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChoiceAction(
    offer: UiOffer,
    args: OfferArgs,
    look: CardActionLook,
    armed: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let args = use_signal(move || args);
    let current = args.read().clone();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:py-1",
            OfferParamsForm { offer: offer.clone(), args }
            OfferPressButton {
                offer,
                args: current,
                variant: look.variant(),
                armed_preview: armed,
                on_action,
            }
        }
    }
}

/// The slot a picker's trigger sits in: on a flush look it fills the row's
/// height (the popover's own wrapper is an inline grid that centres it).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PickSlot(look: CardActionLook, children: Element) -> Element {
    rsx! {
        div { class: pick_slot_class(look), {children} }
    }
}

/// [`PickSlot`]'s classes.
fn pick_slot_class(look: CardActionLook) -> &'static str {
    match look.flush() {
        true => PICK_SLOT_FLUSH_CLASS,
        false => PICK_SLOT_ROW_CLASS,
    }
}

/// A flush picker: the row's height, never shrinking, its popover wrapper
/// stretched to it.
const PICK_SLOT_FLUSH_CLASS: &str = "tw:grid tw:min-w-0 tw:flex-none tw:self-stretch tw:[&>span]:h-full tw:[&>span]:place-items-stretch";

/// A picker on a details row: the row's width.
const PICK_SLOT_ROW_CLASS: &str =
    "tw:grid tw:min-w-0 tw:[&>span]:w-full tw:[&>span]:place-items-stretch";

/// The press of `action`'s offer with its args, in the card's word and
/// icon — or the offer's own refusal, disabled, saying why.
pub(crate) fn card_press(offer: &UiOffer, action: &UiCardAction) -> UiAction {
    let pressed = pressed_or_refused(offer, &action.args).with_label(action.word.clone());
    match &action.icon {
        Some(icon) => pressed.with_icon(icon.clone()),
        None => pressed,
    }
}

/// A picker's trigger in the card's word and icon, on `look`.
fn verb_trigger(action: &UiCardAction, look: CardActionLook) -> Option<VerbTrigger> {
    Some(VerbTrigger {
        word: action.word.clone(),
        icon: action_icon_name(action.icon.as_deref()),
        class: action_variant_class(look.variant(), false),
    })
}

/// The board pick's chip source: the boot banner, or the board's firmware.
fn chip_source(from_banner: bool) -> ChipSource {
    match from_banner {
        true => ChipSource::BootBanner,
        false => ChipSource::Firmware,
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ActionEnablement, UiOfferTree};

    use super::super::card_test_fixtures::{
        attribute_values, board, card_and_tree, porch_view, render,
    };
    use super::*;
    use crate::core::OffersProvider;

    /// Each draw picks its piece: a press is a button in the card's word, a
    /// project pick and a board pick are a popover's trigger in it, a
    /// choice is the offer's own form with its press — each inside an
    /// `AgentMark` carrying the offer path.
    #[test]
    fn each_draw_picks_its_piece_inside_its_agent_mark() {
        let (_, tree) = card_and_tree(&porch_view());
        let forget = offer(&tree, "forget");
        let push = offer(&tree, "push");
        let path = forget.path.to_string();

        let press = UiCardAction::press(&forget, "Forget this board").with_icon("remove");
        let html = render_action(&tree, press, CardActionLook::MenuItem);
        assert_eq!(
            attribute_values(&html, "data-offer-path"),
            vec![path.clone()]
        );
        assert!(html.contains("Forget this board"), "{html}");

        let pick = UiCardAction::press(&push, "Add a project")
            .drawn(UiActionDraw::ProjectPick { board_id: None });
        let html = render_action(&tree, pick, CardActionLook::BarEnd);
        assert_eq!(
            attribute_values(&html, "data-offer-path"),
            vec![push.path.to_string()]
        );
        assert_eq!(attribute_values(&html, "aria-label"), vec!["Add a project"]);
        assert_eq!(
            attribute_values(&html, "aria-expanded"),
            vec!["false"],
            "a popover's trigger"
        );

        let board_pick = UiCardAction::press(&forget, "Install").drawn(UiActionDraw::BoardPick {
            chip: Some("esp32c6".to_string()),
            chip_from_banner: true,
        });
        let html = render_action(&tree, board_pick, CardActionLook::Primary);
        assert_eq!(
            attribute_values(&html, "data-offer-path"),
            vec![path.clone()]
        );

        let choice = UiCardAction::press(&push, "Put it on").drawn(UiActionDraw::Choice);
        let html = render_action(&tree, choice, CardActionLook::MenuItem);
        assert_eq!(
            attribute_values(&html, "data-offer-path"),
            vec![push.path.to_string()]
        );
        assert!(html.contains("<fieldset"), "the offer's own params: {html}");
    }

    /// An offer the tree no longer holds draws nothing — no button, no mark.
    #[test]
    fn a_missing_offer_draws_nothing() {
        let (_, tree) = card_and_tree(&porch_view());
        let forget = offer(&tree, "forget");
        let gone = UiCardAction::press(&forget, "Forget");
        let html = render_action(&UiOfferTree::new(), gone, CardActionLook::MenuItem);
        assert!(!html.contains("<button"), "{html}");
        assert!(
            attribute_values(&html, "data-offer-path").is_empty(),
            "{html}"
        );
    }

    /// The press wears the card's word and icon and is the offer's own
    /// binding; a disabled offer's press is disabled with the offer's
    /// reason, still in the card's word.
    #[test]
    fn a_press_is_the_offers_own_in_the_cards_word() {
        let (_, tree) = card_and_tree(&porch_view());
        let forget = offer(&tree, "forget");
        let action = UiCardAction::press(&forget, "Forget this board").with_icon("remove");
        let pressed = card_press(&forget, &action);
        assert_eq!(pressed.meta().label, "Forget this board");
        assert_eq!(pressed.meta().icon.as_deref(), Some("remove"));
        assert_eq!(
            pressed.offer_press().map(|press| press.path.clone()),
            Some(forget.path.clone()),
            "the press is the offer's"
        );

        let refused = UiOffer::new(
            forget.path.clone(),
            "remove",
            forget.action.clone().disabled("Not while it updates"),
        );
        let pressed = card_press(&refused, &UiCardAction::press(&refused, "Forget"));
        assert_eq!(pressed.meta().label, "Forget");
        assert!(matches!(
            &pressed.meta().enablement,
            ActionEnablement::Disabled { reason } if reason.contains("Not while it updates")
        ));
    }

    /// A look maps to its variant and its picker slot; a picker's trigger
    /// is the card's word and icon on the look's class.
    #[test]
    fn a_look_is_a_variant_and_a_pickers_trigger_wears_it() {
        let looks = [
            (
                CardActionLook::Primary,
                ActionButtonVariant::RowPrimary,
                PICK_SLOT_FLUSH_CLASS,
            ),
            (
                CardActionLook::BarEnd,
                ActionButtonVariant::RowEnd,
                PICK_SLOT_FLUSH_CLASS,
            ),
            (
                CardActionLook::MenuItem,
                ActionButtonVariant::MenuItem,
                PICK_SLOT_ROW_CLASS,
            ),
        ];
        for (look, variant, slot) in looks {
            assert_eq!(look.variant(), variant);
            assert_eq!(pick_slot_class(look), slot);
        }
        let (_, tree) = card_and_tree(&porch_view());
        let action = UiCardAction::press(&offer(&tree, "forget"), "Install").with_icon("firmware");
        let trigger = verb_trigger(&action, CardActionLook::Primary).expect("a trigger");
        assert_eq!(trigger.word, "Install");
        assert!(trigger.icon.is_some());
        assert_eq!(
            trigger.class,
            action_variant_class(ActionButtonVariant::RowPrimary, false)
        );
        assert_eq!(chip_source(true), ChipSource::BootBanner);
        assert_eq!(chip_source(false), ChipSource::Firmware);
    }

    /// The board's offer for `verb`.
    fn offer(tree: &UiOfferTree, verb: &str) -> UiOffer {
        tree.get(&board().child(verb))
            .cloned()
            .unwrap_or_else(|| panic!("`{verb}` is offered"))
    }

    /// `action` drawn on `look` under `tree`, as markup.
    fn render_action(tree: &UiOfferTree, action: UiCardAction, look: CardActionLook) -> String {
        render(
            ActionRoot,
            ActionRootProps {
                tree: tree.clone(),
                action,
                look,
            },
        )
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn ActionRoot(tree: UiOfferTree, action: UiCardAction, look: CardActionLook) -> Element {
        rsx! {
            OffersProvider { offers: tree,
                CardAction { action, look, on_action: |_| {} }
            }
        }
    }
}

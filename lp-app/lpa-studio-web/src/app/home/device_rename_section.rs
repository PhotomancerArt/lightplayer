//! The "Rename" section: a board's `rename` offer, its one Text param drawn
//! as one form, on the project card's Rename precedent (a form in a details
//! card, never a dialog). Prefilled with the name the board wears now, so a
//! board wearing the derived "<board> · <Mon D>" is a few keystrokes from a
//! name of its own. Submitting presses the offer with the typed name — the
//! user-stream `SetName` the model persists to the registry (the name is
//! Studio's, never written to the board) — and closes the popover it sits
//! in: a rename is a completed gesture.
//!
//! Drawn in the board card's hardware details (the Rename panel) and in the
//! header session control's device panel, the other place the name is
//! shown. Moved out of the retired device card unchanged.

use dioxus::prelude::*;
use lpa_studio_core::{OfferArgs, RENAME_NAME_PARAM, UiAction, UiOffer};

use crate::app::agent::AgentMark;
use crate::base::{DetailSection, PopoverCloseHandle};
use crate::core::quiet_action_class;

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceRenameSection(
    /// `devices/<board>/rename`.
    offer: UiOffer,
    /// What the device is called right now — the field's starting value.
    title: String,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut value = use_signal(|| title);
    let close = try_consume_context::<PopoverCloseHandle>();
    let path = offer.path.clone();

    rsx! {
        DetailSection { title: Some("Rename".to_string()),
            // The form presses the offer at this path: marked with it, as
            // every action is (inside the section, so the section keeps its
            // divider).
            AgentMark { path,
                form {
                    class: "tw:flex tw:gap-2",
                    onsubmit: move |event| {
                        event.prevent_default();
                        // A blank name binds nothing: the field stays put.
                        let Some(action) = rename_press(&offer, &value.read()) else {
                            return;
                        };
                        on_action.call(action);
                        if let Some(mut close) = close {
                            close.close();
                        }
                    },
                    input {
                        class: RENAME_INPUT_CLASS,
                        aria_label: "Device name",
                        value: "{value}",
                        oninput: move |event| value.set(event.value()),
                    }
                    button { class: quiet_action_class(), r#type: "submit", "Rename" }
                }
            }
        }
    }
}

/// The offer pressed with `name` as its one param, or nothing when the name
/// does not bind (a blank one).
pub(crate) fn rename_press(offer: &UiOffer, name: &str) -> Option<UiAction> {
    let args = OfferArgs::new().with(RENAME_NAME_PARAM, name.to_string());
    offer.press(&args).ok()
}

/// The rename form's field — the project card's rename input, verbatim.
const RENAME_INPUT_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::board_card::card_test_fixtures::{board, card_and_tree, porch_view};

    /// Submitting presses the offer with the typed name as its one param;
    /// a blank name binds nothing.
    #[test]
    fn a_rename_presses_the_offer_with_the_typed_name() {
        let (_, tree) = card_and_tree(&porch_view());
        let offer = tree
            .get(&board().child("rename"))
            .cloned()
            .expect("rename is offered");
        let pressed = rename_press(&offer, "Porch lights").expect("a name binds");
        let press = pressed.offer_press().expect("the press is the offer's");
        assert_eq!(press.path, offer.path);
        assert_eq!(press.args.get(RENAME_NAME_PARAM), Some("Porch lights"));
        assert!(
            rename_press(&offer, "   ").is_none(),
            "a blank name binds nothing"
        );
    }
}

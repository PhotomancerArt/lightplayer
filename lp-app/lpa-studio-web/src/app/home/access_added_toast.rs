//! "Yona's Mac can now unlock PLAYFUL choker over Bluetooth." — the toast
//! a USB connect raises when it added keys on its own (plan D6, spike §3).
//!
//! Physical connection is access, so there is no prompt: plugging in adds
//! this browser's key (and the account's, and its passwords, when signed
//! in). The toast names what was added and offers Undo, which removes
//! exactly those — and stops Studio adding them to that device again this
//! session.
//!
//! It sits at the bottom of the page, where a thumb is, and fades itself
//! out after about ten seconds (CSS, `.ux-access-toast`). A new add is a
//! new `generation` and so a new toast, even when it names the same things.

use dioxus::prelude::*;
use lpa_studio_core::{AccessAdded, AccessCommand};

use crate::base::{StudioIcon, StudioIconName};
use crate::core::quiet_action_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AccessAddedToast(
    added: AccessAdded,
    device_name: String,
    on_access: EventHandler<AccessCommand>,
    /// Undo was pressed (the shell hides this generation).
    on_dismiss: EventHandler<()>,
    /// Stories: in flow, not fixed to the viewport.
    #[props(default)]
    inline: bool,
) -> Element {
    let device = added.device;
    let class = if inline {
        "ux-access-toast ux-access-toast-inline"
    } else {
        "ux-access-toast"
    };
    rsx! {
        div {
            key: "{added.generation}",
            class,
            role: "status",
            "aria-live": "polite",
            span { class: "tw:inline-flex tw:flex-none tw:pt-px tw:text-status-good-foreground",
                StudioIcon { name: StudioIconName::AccessDone, size: 16 }
            }
            span { class: "tw:min-w-0 tw:flex-1", {sentence(&added.names, &device_name)} }
            button {
                class: quiet_action_class(),
                r#type: "button",
                onclick: move |_| {
                    on_access.call(AccessCommand::UndoAutoAdd { device });
                    on_dismiss.call(());
                },
                "Undo"
            }
        }
    }
}

/// The toast's sentence: the names in bold, joined the way people list
/// them.
fn sentence(names: &[String], device_name: &str) -> Element {
    let joined = join_names(names);
    rsx! {
        strong { class: "tw:font-bold tw:text-strong-foreground", "{joined}" }
        " can now unlock {device_name} over Bluetooth."
    }
}

/// "A", "A and B", "A, B and C".
pub(crate) fn join_names(names: &[String]) -> String {
    match names {
        [] => "This browser".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_join_the_way_people_list_them() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(join_names(&names(&["Yona's Mac"])), "Yona's Mac");
        assert_eq!(
            join_names(&names(&["Yona's Mac", "Yona's account"])),
            "Yona's Mac and Yona's account"
        );
        assert_eq!(
            join_names(&names(&[
                "Yona's Mac",
                "Yona's account",
                "Yona's play password"
            ])),
            "Yona's Mac, Yona's account and Yona's play password"
        );
    }
}

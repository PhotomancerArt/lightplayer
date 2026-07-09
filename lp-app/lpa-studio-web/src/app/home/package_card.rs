//! A "Your projects" gallery card.

use dioxus::prelude::*;
use lpa_studio_core::{
    ActionConfirmation, ControllerId, HOME_NODE_ID, HomeOp, UiAction, UiPackageCard,
};

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::package_export::export_package_to_download;
use crate::app::home::time_ago::time_ago;
use crate::base::{DetailPopover, DetailSection, PopoverPlacement, StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, menu_item_action_class, quiet_action_class};

/// One package card: thumbnail, name, meta, and the card menu. Clicking the
/// card opens the copy the card *is* — the library head, pushed to the
/// simulator (D13).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PackageCard(
    card: UiPackageCard,
    /// This card's open is in flight.
    #[props(default = false)]
    opening: bool,
    /// Some other open is in flight — clicks are ignored.
    #[props(default = false)]
    busy: bool,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let now = now_secs.unwrap_or_else(platform_now_secs);
    let open_card = card.clone();
    let edited_line = card.last_saved_at.map(|at| time_ago(now, at));
    // the slug IS the title; the thumbnail initial skips its date stamp

    rsx! {
        article {
            class: package_card_class(opening),
            onclick: move |_| {
                if !busy && !opening {
                    on_action.call(home_action(HomeOp::OpenPackage {
                        key: open_card.uid.clone(),
                    }));
                }
            },
            CardThumb { seed: card.uid.clone(), label: card.slug.clone() }
            div { class: "tw:flex tw:items-start tw:justify-between tw:gap-2 tw:p-3",
                div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                    p { class: "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                        "{card.slug}"
                    }
                    if opening {
                        p { class: "tw:m-0 tw:text-xs tw:text-status-working-foreground", "Opening…" }
                    } else {
                        if let Some(edited) = edited_line {
                            p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Edited {edited}" }
                        }
                        if let Some(provenance) = card.provenance.clone() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground", "{provenance}" }
                        }
                        if let Some(device) = card.on_device.clone() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-status-good-foreground",
                                "On {device} ✓"
                            }
                        }
                    }
                }
                span { onclick: move |event| event.stop_propagation(),
                    PackageCardMenu { card: card.clone(), on_action }
                }
            }
        }
    }
}

/// The card menu: rename form plus duplicate / export / delete rows. The
/// rows are `UiAction`s rendered in the shared menu-item context (export is
/// a web-side handler wearing the same classes) — one action vocabulary,
/// one look.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PackageCardMenu(card: UiPackageCard, on_action: EventHandler<UiAction>) -> Element {
    let mut rename_value = use_signal(|| card.slug.clone());
    let rename_uid = card.uid.clone();
    let export_card = card.clone();
    let duplicate = home_action(HomeOp::DuplicatePackage {
        uid: card.uid.clone(),
    });
    let delete = home_action(HomeOp::DeletePackage {
        uid: card.uid.clone(),
    })
    .with_confirmation(ActionConfirmation::new(
        "Delete project",
        format!(
            "Delete \"{}\" and its history from your library?",
            card.slug
        ),
        "Delete",
    ));

    rsx! {
        DetailPopover {
            icon: StudioIconName::More,
            label: "Project actions".to_string(),
            placement: PopoverPlacement::BottomEnd,
            DetailSection { title: Some("Rename".to_string()),
                form {
                    class: "tw:flex tw:gap-2",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let name = rename_value.read().trim().to_string();
                        if !name.is_empty() {
                            on_action.call(home_action(HomeOp::RenamePackage {
                                uid: rename_uid.clone(),
                                name,
                            }));
                        }
                    },
                    input {
                        class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground",
                        value: "{rename_value}",
                        oninput: move |event| rename_value.set(event.value()),
                    }
                    button { class: quiet_action_class(), r#type: "submit", "Rename" }
                }
            }
            DetailSection {
                div { class: "tw:grid tw:gap-0.5",
                    ActionButton {
                        action: duplicate,
                        running: false,
                        variant: ActionButtonVariant::MenuItem,
                        on_action,
                    }
                    button {
                        class: menu_item_action_class(),
                        r#type: "button",
                        title: "Download this project as a zip archive.",
                        onclick: move |_| export_package_to_download(&export_card),
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Download, size: 14 }
                        }
                        span { "Export zip" }
                    }
                    ActionButton {
                        action: delete,
                        running: false,
                        variant: ActionButtonVariant::MenuItem,
                        on_action,
                    }
                }
            }
        }
    }
}

pub(crate) fn home_action(op: HomeOp) -> UiAction {
    UiAction::from_op(ControllerId::new(HOME_NODE_ID), op)
}

fn package_card_class(opening: bool) -> &'static str {
    if opening {
        "tw:cursor-wait tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card"
    } else {
        "tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn platform_now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn platform_now_secs() -> f64 {
    0.0
}

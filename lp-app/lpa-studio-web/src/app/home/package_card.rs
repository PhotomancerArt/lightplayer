//! A "Your projects" gallery card.

use std::cell::RefCell;

use dioxus::prelude::*;
use lpa_studio_core::{
    ActionConfirmation, ControllerId, DEPLOY_NODE_ID, DeployOp, HOME_NODE_ID, HomeOp,
    PreviewSource, SyncRelation, UiAction, UiPackageCard,
};

use lpa_studio_core::core::time_ago::time_ago;

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::package_export::export_package_to_download;
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
    let edited_line = card.last_saved_at.map(|at| time_ago(now, at));
    // the slug IS the title; the thumbnail initial skips its date stamp

    rsx! {
        article {
            class: package_card_class(opening),
            // drag a project onto a device card = deploy dialog pre-filled
            draggable: true,
            ondragstart: {
                let uid = card.uid.clone();
                move |_| set_dragged_project(uid.clone())
            },
            // Opening a card is NAVIGATION, so it is a real <a> to the
            // sim route (D37: the URL points at a runtime — a project
            // always opens on the sim, never a device takeover): plain
            // click rides the hashchange → open path, and cmd/middle-click
            // "open in new tab" works natively. The link stretches over
            // the card (absolute overlay) instead of wrapping it, so the
            // card menu isn't interactive-inside-interactive markup; the
            // menu floats above it (z-order).
            a {
                class: "tw:absolute tw:inset-0 tw:z-[1]",
                href: "#/sim/{card.slug}",
                aria_label: "Open {card.slug}",
                onclick: move |event| {
                    if busy || opening {
                        event.prevent_default();
                    }
                },
            }
            CardThumb {
                seed: card.uid.clone(),
                label: card.slug.clone(),
                source: Some(PreviewSource::ProjectUid(card.uid.clone())),
            }
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
                        // the association parity line yields to the LIVE
                        // indication when the device is actually here
                        if card.connected_device.is_none() {
                            if let Some(device) = card.on_device.clone() {
                                p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-status-good-foreground",
                                    "On {device} ✓"
                                }
                            }
                        }
                        // D28: the runtime-presence chip — device line,
                        // sim line, or the "Live in 2 places" aggregate
                        // when the project runs on BOTH. Chips are
                        // pointers, deliberately inert: no runtime grab
                        // from a project card (D29's never-a-surprise-
                        // takeover); the runtime cards themselves sit one
                        // glance up in the roster.
                        if let Some(live) = live_presence_line(&card) {
                            p { class: live.class, title: live.title, "{live.text}" }
                        }
                        // a fact, not a warning: neutral chip; the card stays
                        // clickable — the open's refusal notice explains
                        if card.open_elsewhere {
                            p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground",
                                span { class: "tw:inline-block tw:rounded tw:border tw:border-border tw:px-1.5 tw:py-px",
                                    "Open in another tab"
                                }
                            }
                        }
                    }
                }
                span {
                    class: "tw:relative tw:z-[2]",
                    PackageCardMenu { card: card.clone(), on_action }
                }
            }
            // the crystallized open action (D36 prep): same navigation as
            // the bare card click, spelled out — projects always open on
            // the sim, never a device takeover (D29)
            div { class: "tw:flex tw:px-3 tw:pb-3",
                a {
                    class: "{quiet_action_class()} tw:relative tw:z-[2]",
                    href: "#/sim/{card.slug}",
                    title: "Open this project in the simulator.",
                    onclick: move |event| {
                        if busy || opening {
                            event.prevent_default();
                        }
                    },
                    span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                        StudioIcon { name: StudioIconName::Play, size: 14 }
                    }
                    span { "Open in sim" }
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

    let push_to_device = card.connected_device.as_ref().map(|connection| {
        UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::OpenDialog {
                target_key: Some(card.uid.clone()),
            },
        )
        .with_label(format!("Push to {}…", connection.device_name))
        .with_summary("Review and push this project to the connected device.")
        .with_icon("upload")
    });

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
                    if let Some(push) = push_to_device {
                        ActionButton {
                            action: push,
                            running: false,
                            variant: ActionButtonVariant::MenuItem,
                            on_action,
                        }
                    }
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
    // tw:relative anchors the stretched open link (see the card markup)
    if opening {
        "tw:relative tw:cursor-wait tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card"
    } else {
        "tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
    }
}

thread_local! {
    /// The project uid mid-drag (HTML5 dataTransfer is awkward through
    /// Dioxus; a same-page hand-off cell is all card→card drag needs).
    static DRAGGED_PROJECT: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) fn set_dragged_project(uid: String) {
    DRAGGED_PROJECT.with(|cell| *cell.borrow_mut() = Some(uid));
}

pub(crate) fn take_dragged_project() -> Option<String> {
    DRAGGED_PROJECT.with(|cell| cell.borrow_mut().take())
}

/// One rendered runtime-presence line (the D28 chip family).
#[derive(Debug, PartialEq, Eq)]
struct LivePresenceLine {
    text: String,
    class: &'static str,
    /// Tooltip spelling out the places on the aggregate line; `None` when
    /// the single line already says everything.
    title: Option<String>,
}

const LIVE_LINE_GOOD: &str = "tw:m-0 tw:truncate tw:text-xs tw:text-status-good-foreground";
const LIVE_LINE_ATTENTION: &str = "tw:m-0 tw:truncate tw:text-xs tw:text-status-working-foreground";

/// The card's runtime-presence line (D28, full semantics):
///
/// - live device only → the D24 connected line (green only when current —
///   green = good; behind/diverged read as facts needing attention);
/// - sim only → "Running in simulator" (load-as-push always runs the
///   head, so the sim is current: green);
/// - BOTH → the aggregate "Live in 2 places" (the pool cap makes 2 the
///   max for now), amber whenever the device side needs attention, with
///   the tooltip spelling the places out.
fn live_presence_line(card: &UiPackageCard) -> Option<LivePresenceLine> {
    match (card.connected_device.as_ref(), card.running_in_sim) {
        (Some(connection), true) => Some(LivePresenceLine {
            text: "Live in 2 places".to_string(),
            class: match connection.relation {
                SyncRelation::AtHead => LIVE_LINE_GOOD,
                SyncRelation::Behind | SyncRelation::Diverged => LIVE_LINE_ATTENTION,
            },
            title: Some(format!(
                "{} · running in simulator",
                connected_line(&connection.device_name, connection.relation)
            )),
        }),
        (Some(connection), false) => Some(LivePresenceLine {
            text: connected_line(&connection.device_name, connection.relation),
            class: match connection.relation {
                SyncRelation::AtHead => LIVE_LINE_GOOD,
                SyncRelation::Behind | SyncRelation::Diverged => LIVE_LINE_ATTENTION,
            },
            title: None,
        }),
        (None, true) => Some(LivePresenceLine {
            text: "Running in simulator".to_string(),
            class: LIVE_LINE_GOOD,
            title: None,
        }),
        (None, false) => None,
    }
}

/// The D24 connected indication: green only when the device is current
/// (green = good); behind/diverged read as facts needing attention.
fn connected_line(device_name: &str, relation: SyncRelation) -> String {
    match relation {
        SyncRelation::AtHead => format!("On {device_name} — connected ✓"),
        SyncRelation::Behind => format!("On {device_name} — behind your copy"),
        SyncRelation::Diverged => format!("On {device_name} — edited elsewhere"),
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

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiCardConnection;

    use super::*;

    fn card(connected: Option<SyncRelation>, running_in_sim: bool) -> UiPackageCard {
        UiPackageCard {
            uid: "prj_1".to_string(),
            kind: "Project".to_string(),
            slug: "2026-07-09-1421-basic".to_string(),
            last_saved_at: None,
            provenance: None,
            on_device: None,
            open_elsewhere: false,
            connected_device: connected.map(|relation| UiCardConnection {
                device_name: "Porch sign".to_string(),
                relation,
            }),
            running_in_sim,
        }
    }

    #[test]
    fn both_runtimes_aggregate_to_live_in_2_places() {
        // D28 aggregate: one line, not two — the pool cap makes 2 the max
        let line = live_presence_line(&card(Some(SyncRelation::AtHead), true)).unwrap();
        assert_eq!(line.text, "Live in 2 places");
        assert_eq!(line.class, LIVE_LINE_GOOD, "both places current = good");
        assert_eq!(
            line.title.as_deref(),
            Some("On Porch sign — connected ✓ · running in simulator"),
            "the tooltip spells the places out"
        );
    }

    #[test]
    fn a_behind_device_turns_the_aggregate_amber() {
        let line = live_presence_line(&card(Some(SyncRelation::Behind), true)).unwrap();
        assert_eq!(line.text, "Live in 2 places");
        assert_eq!(
            line.class, LIVE_LINE_ATTENTION,
            "one place needing attention colors the aggregate"
        );
    }

    #[test]
    fn single_runtimes_keep_their_own_lines() {
        let device = live_presence_line(&card(Some(SyncRelation::Behind), false)).unwrap();
        assert_eq!(device.text, "On Porch sign — behind your copy");
        assert_eq!(device.class, LIVE_LINE_ATTENTION);
        assert_eq!(device.title, None);

        let sim = live_presence_line(&card(None, true)).unwrap();
        assert_eq!(sim.text, "Running in simulator");
        assert_eq!(sim.class, LIVE_LINE_GOOD);

        assert_eq!(live_presence_line(&card(None, false)), None);
    }
}

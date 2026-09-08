//! The mismatch page: you asked to open a project on a device that is
//! running a different one.
//!
//! **Never a silent push** (vision D50). The ordinary open ends in a push,
//! and a push replaces what is running. When the address named a device
//! (`?on=mac:<base mac>`) and that device is already running another
//! project, the open stops here instead of taking the decision: both
//! projects are named, and the two things a person could have meant are
//! offered as verbs.
//!
//! - **Switch to <running>** — go to the project that is actually
//!   running. A navigation; the device is untouched.
//! - **Push <this> here** — replace what is running, now that the
//!   consequence has been read.
//!
//! The page draws the real cards, not descriptions of them: the device's
//! own roster card (the fold's live view, feed and all) and the running
//! project's gallery card. Nothing here is a second rendering of a device
//! that could drift from the Devices page.
//!
//! # What "backing up first" means here (Q7)
//!
//! The plan asked for the running project to be **pulled off the device
//! into the library before the push**. There is no pull: `Action::Pull`,
//! `EffectRequest::Pull` and `ActivityKind::Pull` are the round-2 (M4)
//! variants that never landed, and building them is a model phase, not
//! this one.
//!
//! It turns out the page does not need one. This page can only be drawn
//! when Studio can *name* the running project, and it can only name it
//! from the library — so by construction, everything the page offers to
//! push over is already in the library. The backup is not skipped; it is
//! already satisfied. A device running something this library does not
//! have gets no verbs at all rather than a push with nothing behind it.
//! (A board whose copy has been edited since it was pushed is the case
//! neither this page nor any other surface covers today, and it is the
//! question the advanced version of this page should answer.)
//!
//! # Plain on purpose
//!
//! This page has no spike. Yona ruled (2026-09-07): basic implementation
//! first, the UX conversation afterwards. It is the house grammar — a
//! heading, two cards, a verb row — and G2 is where it gets designed.

use dioxus::prelude::*;
use lpa_studio_core::{
    HOME_NODE_ID, HomeOp, UiAction, UiHomeView, UiOpenMismatch, UiRunningProject,
};

use crate::app::home::device_roster_card::DeviceRosterCard;
use crate::app::home::package_card::PackageCard;
use crate::core::solid_action_class;
use crate::device_hint::DeviceHint;
use crate::router::{ProjectView, StudioRoute};
use lpa_studio_core::ActionPriority;

/// The route-framed mismatch state (D50), rendered in place of the opening
/// frame.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MismatchPage(
    mismatch: UiOpenMismatch,
    /// The gallery the two cards come from. `None` (the gallery view has
    /// not been built) falls back to naming both by their words alone —
    /// the page still says the true thing and still offers the verbs.
    #[props(default)]
    home: Option<UiHomeView>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let hint = DeviceHint::Mac(mismatch.device_base_mac.clone());
    let device_card = home.as_ref().and_then(|home| {
        home.devices
            .roster
            .devices
            .iter()
            .find(|card| Some(&mismatch.device_key) == home.devices.open_addresses.get(&card.id.0))
            .cloned()
    });
    let running_card = mismatch.running.as_ref().and_then(|running| {
        home.as_ref()
            .and_then(|home| home.projects.iter().find(|card| card.uid == running.uid))
            .cloned()
    });
    // The whole of what this page is for: the person read what is running
    // and chose to replace it.
    let push_here = UiAction::from_op(
        HOME_NODE_ID,
        HomeOp::OpenPackageOnDevice {
            key: mismatch.project_uid.clone(),
            base_mac: mismatch.device_base_mac.clone(),
            over_running_project: true,
        },
    );
    // Switch is a plain in-app link, like every other navigation in the
    // app: the route listener turns the click into a history push and the
    // ordinary open, and cmd-click still opens a real new tab.
    let switch_href = mismatch
        .running
        .as_ref()
        .and_then(|running| switch_href(running, &hint));

    rsx! {
        section { class: "tw:grid tw:max-w-[860px] tw:gap-5",
            header { class: "tw:grid tw:gap-2",
                h1 { class: "tw:m-0 tw:text-lg tw:font-semibold tw:text-strong-foreground",
                    "{mismatch.device_name} is running something else"
                }
                p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-muted-foreground",
                    {headline(&mismatch)}
                }
            }

            div { class: "tw:grid tw:gap-3.5 tw:grid-cols-2 tw:max-[860px]:grid-cols-1",
                // The device, as the Devices page draws it — the fold's
                // live view, never a second telling of it.
                match device_card {
                    Some(card) => rsx! {
                        DeviceRosterCard {
                            key: "mismatch-device-{card.id.0}",
                            open_uid: Some(mismatch.device_key.clone()),
                            feed: home.as_ref().and_then(|home| home.devices.feeds.get(&card.id).cloned()),
                            runtime: home.as_ref().and_then(|home| home.devices.runtime_bands.get(&card.id).cloned()),
                            card,
                            projects: home.as_ref().map(|home| home.projects.clone()).unwrap_or_default(),
                            examples: home.as_ref().map(|home| home.examples.clone()).unwrap_or_default(),
                            on_action,
                        }
                    },
                    None => rsx! {
                        NamedThing {
                            label: "The device",
                            name: mismatch.device_name.clone(),
                            detail: mismatch.device_base_mac.clone(),
                        }
                    },
                }
                // What it is running.
                match (running_card, mismatch.running.clone()) {
                    (Some(card), _) => rsx! {
                        PackageCard { key: "mismatch-running-{card.uid}", card, on_action }
                    },
                    (None, Some(running)) => rsx! {
                        NamedThing {
                            label: "Running on it",
                            name: running.name,
                            detail: running.uid,
                        }
                    },
                    (None, None) => rsx! {
                        NamedThing {
                            label: "Running on it",
                            name: "A project that isn't in your library".to_string(),
                            detail: String::new(),
                        }
                    },
                }
            }

            div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2.5",
                if let (Some(href), Some(running)) = (switch_href, mismatch.running.clone()) {
                    a {
                        class: solid_action_class(ActionPriority::Primary),
                        href: "{href}",
                        "Switch to {running.name}"
                    }
                    button {
                        r#type: "button",
                        class: solid_action_class(ActionPriority::Secondary),
                        onclick: move |_| on_action.call(push_here.clone()),
                        "Push {mismatch.project_name} here"
                    }
                }
                a {
                    class: "tw:text-sm tw:text-muted-foreground tw:no-underline tw:hover:text-strong-foreground",
                    href: "/devices",
                    "Devices"
                }
            }
        }
    }
}

/// The sentence under the heading: what is where, and what each verb
/// costs. Two shapes, because a project this library cannot name has no
/// switch to offer and nothing standing behind a push.
fn headline(mismatch: &UiOpenMismatch) -> String {
    match &mismatch.running {
        Some(running) => format!(
            "{} is on it, not {}. Switching leaves the device alone. Pushing replaces {} — your library keeps it.",
            running.name, mismatch.project_name, running.name,
        ),
        None => format!(
            "It is running a project your library doesn't have. There is nothing to switch to, and nothing to put back if {} replaced it.",
            mismatch.project_name,
        ),
    }
}

/// The address of the project that IS running, carrying the same device
/// hint — what changes is the address, not the device.
///
/// `None` for a uid that will not parse, which is a project the library
/// could not have handed us in the first place.
fn switch_href(running: &UiRunningProject, hint: &DeviceHint) -> Option<String> {
    Some(
        StudioRoute::Project {
            uid: running.uid.parse().ok()?,
            slug: Some(running.name.clone()),
            view: ProjectView::Workspace,
            on: Some(hint.clone()),
        }
        .path(),
    )
}

/// The fallback for a card the gallery could not hand over: the thing,
/// named, in the house's own quiet grammar.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NamedThing(label: &'static str, name: String, detail: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-1 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4",
            p { class: "tw:m-0 tw:text-xs tw:uppercase tw:tracking-wide tw:text-muted-foreground", "{label}" }
            p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground", "{name}" }
            if !detail.is_empty() {
                p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-muted-foreground", "{detail}" }
            }
        }
    }
}

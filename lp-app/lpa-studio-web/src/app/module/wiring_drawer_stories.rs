//! Stories for the **wiring** drawer — the bus channel rows in their new
//! home.
//!
//! These are the retired `studio/bus/bus-pane/*` stories, relocated rather
//! than redesigned (P3): the sidebar bus pane is gone, and
//! [`crate::app::WiringDrawerBody`] now renders inside the module card's wiring
//! drawer, one view per scope, hung off the module that owns it. The rows
//! themselves — the violet slot-pane treatment, the PRIMARY badge, the
//! writers→readers detail popup — are unchanged, which is exactly what
//! these stories keep pinned.
//!
//! Framing note: the drawer is shown on a module face with an EMPTY panel
//! so the channel rows are the subject. The drawer in a full workspace
//! card is `studio/module/module-face/wiring-drawer-open`.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, ProjectEditorOp, UiAction, UiBusChannelView, UiBusSiteView, UiBusView,
    UiModuleFace, UiPanelGroup,
};
use lpa_studio_web_story_macros::story;

use crate::app::wiring::wiring_drawer::BusChannelPane;

use super::ModuleFace;
use super::module_fixtures::{ROOT_SCOPE, scope_target};

fn site(label: &str, slot: Option<&str>, default_origin: bool) -> UiBusSiteView {
    UiBusSiteView {
        node_label: label.to_string(),
        slot: slot.map(str::to_string),
        default_origin,
        focus: Some(UiAction::from_op(
            ControllerId::new("story.module"),
            ProjectEditorOp::Focus,
        )),
    }
}

/// A fyeah-sign-shaped scope: standard channels, multi-writer trigger, a
/// default-origin clock writer, and the highlighted primary visual.
fn fyeah_bus_view() -> UiBusView {
    let scope = Some(scope_target(ROOT_SCOPE));
    UiBusView {
        channels: vec![
            UiBusChannelView {
                scope,
                scope_label: None,
                name: "time".to_string(),
                kind: Some("Instant".to_string()),
                value: Some("3.333".to_string()),
                value_error: None,
                primary_visual: false,
                writers: vec![site("Clock", Some("seconds"), true)],
                readers: vec![site("Playlist", Some("time"), false)],
            },
            UiBusChannelView {
                scope,
                scope_label: None,
                name: "trigger".to_string(),
                kind: Some("Instant".to_string()),
                value: None,
                value_error: None,
                primary_visual: false,
                writers: vec![
                    site("Button", Some("down"), false),
                    site("Radio", Some("output"), false),
                ],
                readers: vec![
                    site("Playlist", Some("trigger"), false),
                    site("Radio", Some("input"), false),
                ],
            },
            UiBusChannelView {
                scope,
                scope_label: None,
                name: "visual.out".to_string(),
                kind: Some("Color".to_string()),
                value: Some("visual product #5:0".to_string()),
                value_error: None,
                primary_visual: true,
                writers: vec![site("Playlist", Some("output"), true)],
                readers: vec![site("Fixture", Some("input"), false)],
            },
            UiBusChannelView {
                scope,
                scope_label: None,
                name: "control.out".to_string(),
                kind: Some("Color".to_string()),
                value: None,
                value_error: Some("no provider produced a value".to_string()),
                primary_visual: false,
                writers: vec![site("Fixture", Some("output"), false)],
                readers: vec![site("Output", Some("input"), false)],
            },
        ],
    }
}

/// A module face that is nothing but its open wiring drawer: no hero, an
/// empty panel, so the channel rows carry the story.
fn drawer_only_face(wiring: UiBusView) -> UiModuleFace {
    UiModuleFace {
        preview: None,
        panel: UiPanelGroup::new("Aurora Sign", ROOT_SCOPE).with_target(scope_target(ROOT_SCOPE)),
        wiring: Some(wiring),
        wiring_open: true,
        provenance: None,
        auto_save: None,
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DrawerCard(face: UiModuleFace) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-[720px] tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            ModuleFace { face, on_action: move |_| {} }
        }
    }
}

#[story(
    label = "Fyeah Sign",
    description = "The wiring drawer open on a scope with a multi-writer trigger, a default-origin clock writer, an unresolved channel, and the highlighted primary visual. This is the retired sidebar bus pane's content, unchanged, in its new home on the module that owns the scope."
)]
pub(crate) fn fyeah_sign() -> Element {
    rsx! {
        DrawerCard { face: drawer_only_face(fyeah_bus_view()) }
    }
}

#[story(
    label = "Channel Detail",
    description = "The channel detail popup: info section plus wiring — every writer/reader is a clickable focus chip, multi-writer channels spell out merge semantics, default-origin sites are marked. The popup is unchanged by the move off the sidebar."
)]
pub(crate) fn channel_detail() -> Element {
    let channel = fyeah_bus_view().channels.remove(1);
    rsx! {
        div { class: "tw:flex tw:min-h-[460px] tw:max-w-72 tw:flex-col",
            BusChannelPane {
                channel,
                on_action: |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "A module that publishes nothing: both the panel and the wiring drawer explain that binding a slot to `bus:…` is what makes a channel public. The drawer is Some-with-no-rows, never absent — an empty scope is a real shape, not a loading state."
)]
pub(crate) fn empty() -> Element {
    rsx! {
        DrawerCard { face: drawer_only_face(UiBusView::empty()) }
    }
}

//! Stories for the **wiring** drawer — the flow view.
//!
//! The drawer reads `[writers] → [value box] → [readers]`, one row per
//! channel of the module's scope (the 2026-08-03 wiring-UI spike,
//! `spikes/wiring-ui/index.html`, gates G1–G3 — the retired bus-pane
//! value-hero rows are gone). These stories pin the row grammar, the
//! wire/arrowhead treatments, and every binding flavor the flow view
//! draws: default-origin, engaged panel writers, module publishes,
//! shadowed writers, E3 contention, child-scope readers, R6
//! invitations, and unresolved channels.
//!
//! Framing note: the drawer is shown on a module face with an EMPTY
//! panel so the flow rows are the subject. The drawer in a full
//! workspace card is `studio/module/module-face/wiring-drawer-open`.

use dioxus::prelude::*;
use lpa_studio_core::app::project::format_gradient_summary;
use lpa_studio_core::{
    ControllerId, ProjectEditorOp, UiAction, UiBusChannelPreview, UiBusChannelView,
    UiBusSiteOrigin, UiBusSiteView, UiBusView, UiModuleFace, UiPanelGroup, UiProductKind,
    UiProductPreview, UiProductPreviewFrame, UiProductTrackingState,
};
use lpa_studio_web_story_macros::story;
use lpc_model::GradientConfig;

use crate::app::node::node_story_fixtures::{
    control_preview_product, palette_cycle, sunset_gradient, visual_preview_bytes,
};
use crate::app::wiring::wiring_drawer::FlowChannelRow;

use super::ModuleFace;
use super::module_fixtures::{ROOT_SCOPE, scope_target};

fn site(label: &str, slot: Option<&str>, origin: UiBusSiteOrigin) -> UiBusSiteView {
    UiBusSiteView {
        node_label: label.to_string(),
        slot: slot.map(str::to_string),
        origin,
        publish: false,
        shadowed: false,
        child_scope: None,
        focus: Some(UiAction::from_op(
            ControllerId::new("story.module"),
            ProjectEditorOp::Focus,
        )),
    }
}

fn channel(name: &str, kind: &str) -> UiBusChannelView {
    UiBusChannelView {
        scope: Some(scope_target(ROOT_SCOPE)),
        scope_label: None,
        name: name.to_string(),
        kind: Some(kind.to_string()),
        value: None,
        value_error: None,
        primary_visual: false,
        contended: false,
        preview: None,
        gradient: None,
        writers: Vec::new(),
        readers: Vec::new(),
    }
}

fn visual_channel_preview() -> UiBusChannelPreview {
    UiBusChannelPreview {
        kind: UiProductKind::Visual,
        preview: UiProductPreview::VisualSrgb8 {
            width: 32,
            height: 32,
            revision: 104,
            bytes: visual_preview_bytes(32, 32).into(),
        },
        tracking: UiProductTrackingState::Tracking,
        frame: UiProductPreviewFrame::VISUAL_DEFAULT,
    }
}

/// A control channel's value box: the fixture's lamp layout, from the same
/// preview payload the visual rows carry with the control family on it.
fn control_channel_preview() -> UiBusChannelPreview {
    UiBusChannelPreview {
        kind: UiProductKind::Control,
        preview: control_preview_product("output").preview,
        tracking: UiProductTrackingState::Tracking,
        frame: UiProductPreviewFrame::VISUAL_DEFAULT,
    }
}

/// A fyeah-sign-shaped scope: standard channels, a multi-writer trigger,
/// a default-origin clock writer, an unresolved channel, and the
/// highlighted primary visual with its live picture.
fn fyeah_bus_view() -> UiBusView {
    UiBusView {
        channels: vec![
            UiBusChannelView {
                value: Some("3.333".to_string()),
                writers: vec![site("Clock", Some("seconds"), UiBusSiteOrigin::Default)],
                readers: vec![site("Playlist", Some("time"), UiBusSiteOrigin::Authored)],
                ..channel("time", "Instant")
            },
            UiBusChannelView {
                writers: vec![
                    site("Button", Some("down"), UiBusSiteOrigin::Authored),
                    site("Radio", Some("output"), UiBusSiteOrigin::Authored),
                ],
                readers: vec![
                    site("Playlist", Some("trigger"), UiBusSiteOrigin::Authored),
                    site("Radio", Some("input"), UiBusSiteOrigin::Authored),
                ],
                ..channel("trigger", "Instant")
            },
            UiBusChannelView {
                value: Some("visual product #5:0".to_string()),
                primary_visual: true,
                preview: Some(visual_channel_preview()),
                writers: vec![site("Playlist", Some("output"), UiBusSiteOrigin::Default)],
                readers: vec![site("Fixture", Some("input"), UiBusSiteOrigin::Authored)],
                ..channel("visual.out", "Color")
            },
            UiBusChannelView {
                value_error: Some("no provider produced a value".to_string()),
                writers: vec![site("Fixture", Some("output"), UiBusSiteOrigin::Authored)],
                readers: vec![site("Output", Some("input"), UiBusSiteOrigin::Authored)],
                ..channel("control.out", "Color")
            },
        ],
    }
}

/// Every flavor the flow view draws, in one scope: an engaged panel
/// writer over a shadowed authored one, E3 contention between two module
/// publishes, child-scope readers inheriting through R5, an R6
/// invitation, and a unit-float position bar.
fn states_bus_view() -> UiBusView {
    let child_reader = |scope: &str, slot: &str| UiBusSiteView {
        child_scope: Some(scope.to_string()),
        ..site("sim", Some(slot), UiBusSiteOrigin::Authored)
    };
    UiBusView {
        channels: vec![
            UiBusChannelView {
                value: Some("0.735".to_string()),
                writers: vec![
                    site("panel", None, UiBusSiteOrigin::Panel),
                    UiBusSiteView {
                        shadowed: true,
                        ..site("Master hue", Some("value"), UiBusSiteOrigin::Authored)
                    },
                ],
                readers: vec![
                    child_reader("plasma_1", "hue"),
                    child_reader("plasma_2", "hue"),
                ],
                ..channel("hue", "Float")
            },
            UiBusChannelView {
                writers: Vec::new(),
                readers: vec![site("halo", Some("brightness"), UiBusSiteOrigin::Authored)],
                ..channel("brightness", "Float")
            },
            UiBusChannelView {
                value: Some("visual product #9:0".to_string()),
                primary_visual: true,
                contended: true,
                preview: Some(visual_channel_preview()),
                writers: vec![
                    UiBusSiteView {
                        publish: true,
                        ..site("plasma_1", Some("visual.out"), UiBusSiteOrigin::Default)
                    },
                    UiBusSiteView {
                        publish: true,
                        ..site("plasma_2", Some("visual.out"), UiBusSiteOrigin::Default)
                    },
                ],
                readers: vec![site("halo", Some("input"), UiBusSiteOrigin::Authored)],
                ..channel("visual.out", "Color")
            },
        ],
    }
}

/// The output half of a sign's scope: the visual the fixture samples, and
/// the control product it renders for the hardware output. Both value boxes
/// show their picture — pixels and lamps — because both are products in the
/// tracked preview stream.
fn output_bus_view() -> UiBusView {
    UiBusView {
        channels: vec![
            UiBusChannelView {
                value: Some("visual product #5:0".to_string()),
                primary_visual: true,
                preview: Some(visual_channel_preview()),
                writers: vec![site("Playlist", Some("output"), UiBusSiteOrigin::Default)],
                readers: vec![site("Fixture", Some("input"), UiBusSiteOrigin::Authored)],
                ..channel("visual.out", "Color")
            },
            UiBusChannelView {
                value: Some("control product #7:0".to_string()),
                preview: Some(control_channel_preview()),
                writers: vec![site("Fixture", Some("output"), UiBusSiteOrigin::Default)],
                readers: vec![site("Output", Some("input"), UiBusSiteOrigin::Default)],
                ..channel("control.out", "Color")
            },
        ],
    }
}

/// A module face that is nothing but its open wiring drawer: no hero, an
/// empty panel, so the flow rows carry the story.
fn drawer_only_face(wiring: UiBusView) -> UiModuleFace {
    UiModuleFace {
        preview: None,
        hero_choice: None,
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
    description = "The wiring drawer as the flow view: writer chips enter on arrowed wires, the channel's value box is the junction (the primary visual shows its picture), readers fan out right. A default-origin clock writer (dashed chip), a two-writer trigger, and an unresolved channel."
)]
pub(crate) fn fyeah_sign() -> Element {
    rsx! {
        DrawerCard { face: drawer_only_face(fyeah_bus_view()) }
    }
}

#[story(
    description = "Every binding flavor in one scope: an engaged panel writer (orange) shadowing an authored write (faded chip, dashed wire), E3 contention between two module publishes (attention box + 2× fallback badge — display only, the pick gesture is future work), child-scope readers inheriting through R5 (dotted chips with their scope path), an R6 invitation with no writer, and a unit-float position bar under the value."
)]
pub(crate) fn states() -> Element {
    rsx! {
        DrawerCard { face: drawer_only_face(states_bus_view()) }
    }
}

#[story(
    label = "Product Channels",
    description = "The two product channels of a sign's scope, side by side: `visual.out` shows its pixels, `control.out` shows the fixture's lamp layout. A control channel is a picture too — its value box renders the same lamps the fixture card does, instead of the `control product #7:0` string the flow view used to fall back to."
)]
pub(crate) fn product_channels() -> Element {
    rsx! {
        DrawerCard { face: drawer_only_face(output_bus_view()) }
    }
}

#[story(
    description = "Palette channels in the flow view: a held palette and a cycle's member set draw in their value boxes, for the same reason product channels draw their pixels — the value box shows what is on the channel. The summary line under each strip carries the words the box has no room to spell."
)]
pub(crate) fn gradient_channels() -> Element {
    rsx! {
        DrawerCard { face: drawer_only_face(gradient_bus_view()) }
    }
}

/// A palette-carrying scope: one held palette, one cycle.
fn gradient_bus_view() -> UiBusView {
    let palette_channel = |name: &str, config: GradientConfig| UiBusChannelView {
        value: Some(format_gradient_summary(&config)),
        gradient: Some(config),
        writers: vec![site("Palette", Some("output"), UiBusSiteOrigin::Authored)],
        readers: vec![site("Plasma", Some("palette"), UiBusSiteOrigin::Authored)],
        ..channel(name, "Color")
    };
    UiBusView {
        channels: vec![
            palette_channel("palette", GradientConfig::Static(sunset_gradient())),
            palette_channel("palette.cycle", palette_cycle()),
        ],
    }
}

#[story(
    label = "Channel Detail",
    description = "The channel detail popup, opened from the value box: info section plus wiring — every writer/reader is a clickable focus row, with flavors (default origin, panel writer, shadowed, child scope) spelled out. The popup carries what the flow row abbreviates."
)]
pub(crate) fn channel_detail() -> Element {
    let channel = fyeah_bus_view().channels.remove(1);
    rsx! {
        div { class: "tw:flex tw:min-h-[460px] tw:w-full tw:max-w-[560px] tw:flex-col",
            FlowChannelRow {
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

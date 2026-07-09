//! Stories for the bus pane body.

use dioxus::prelude::*;
use lpa_studio_core::{UiBusChannelView, UiBusSiteView, UiBusView};
use lpa_studio_web_story_macros::story;

use crate::app::bus::BusPaneBody;

fn site(label: &str, slot: Option<&str>, default_origin: bool) -> UiBusSiteView {
    UiBusSiteView {
        node_label: label.to_string(),
        slot: slot.map(str::to_string),
        default_origin,
        focus: None,
    }
}

/// A fyeah-sign-shaped bus: standard channels, multi-writer trigger, a
/// default-origin clock writer, and the highlighted primary visual.
fn fyeah_bus_view() -> UiBusView {
    UiBusView {
        channels: vec![
            UiBusChannelView {
                name: "time".to_string(),
                kind: Some("Instant".to_string()),
                value: Some("3.333".to_string()),
                value_error: None,
                primary_visual: false,
                writers: vec![site("Clock", Some("seconds"), true)],
                readers: vec![site("Playlist", Some("time"), false)],
            },
            UiBusChannelView {
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
                name: "visual.out".to_string(),
                kind: Some("Color".to_string()),
                value: Some("visual product #5:0".to_string()),
                value_error: None,
                primary_visual: true,
                writers: vec![site("Playlist", Some("output"), true)],
                readers: vec![site("Fixture", Some("input"), false)],
            },
            UiBusChannelView {
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

#[story(
    label = "Fyeah Sign",
    description = "Channels with multi-writer trigger, default-origin writers, and the primary visual highlight."
)]
pub(crate) fn fyeah_sign() -> Element {
    rsx! {
        div { class: "tw:max-w-72",
            BusPaneBody {
                view: fyeah_bus_view(),
                on_action: |_| {},
            }
        }
    }
}

#[story(description = "Empty state explaining the bus concept.")]
pub(crate) fn empty() -> Element {
    rsx! {
        div { class: "tw:max-w-72",
            BusPaneBody {
                view: UiBusView::empty(),
                on_action: |_| {},
            }
        }
    }
}

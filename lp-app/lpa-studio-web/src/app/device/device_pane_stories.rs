//! Stories for the Studio device pane.

use dioxus::prelude::*;
use lpa_studio_core::UiLogLevel;
use lpa_studio_web_story_macros::story;

use crate::app::story_fixtures::{
    browser_serial_blank_firmware_view, browser_serial_canceled_view,
    browser_serial_open_failed_view, idle_device_view, lightplayer_disconnected_view,
    provision_failed_view, provision_ready_view, provisioning_view, reset_complete_view,
    resetting_to_blank_view, shell_story, studio_log,
};
use crate::core::PaneView;

#[story]
pub(crate) fn device_pane() -> Element {
    let view = idle_device_view();
    rsx! {
        AppPane {
            view,
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn browser_serial_canceled() -> Element {
    shell_story(browser_serial_canceled_view(), false, Vec::new())
}

#[story]
pub(crate) fn browser_serial_open_failed() -> Element {
    shell_story(browser_serial_open_failed_view(), false, Vec::new())
}

#[story]
pub(crate) fn server_disconnected_link_ready() -> Element {
    shell_story(
        lightplayer_disconnected_view(),
        false,
        vec![studio_log(UiLogLevel::Info, "LightPlayer disconnected")],
    )
}

#[story]
pub(crate) fn provision_ready() -> Element {
    shell_story(provision_ready_view(), false, Vec::new())
}

#[story]
pub(crate) fn browser_serial_blank_firmware() -> Element {
    shell_story(browser_serial_blank_firmware_view(), false, Vec::new())
}

#[story]
pub(crate) fn provisioning() -> Element {
    shell_story(provisioning_view(), true, Vec::new())
}

#[story]
pub(crate) fn provision_failed() -> Element {
    shell_story(
        provision_failed_view(),
        false,
        vec![studio_log(
            UiLogLevel::Error,
            "browser serial firmware flashing failed",
        )],
    )
}

#[story]
pub(crate) fn resetting_to_blank() -> Element {
    shell_story(resetting_to_blank_view(), true, Vec::new())
}

#[story]
pub(crate) fn reset_complete() -> Element {
    shell_story(
        reset_complete_view(),
        false,
        vec![studio_log(UiLogLevel::Info, "ESP32-C6 wiped")],
    )
}

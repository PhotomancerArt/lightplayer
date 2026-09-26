//! The Bluetooth access surfaces' wiring, as a context.
//!
//! The device card's Connections group and "Who has access", and the
//! Devices page's access settings, sit several layers under the shell; the
//! web app provides their callback and the view slice they read here
//! instead of threading them through every component between. Stories
//! provide none, so those surfaces render inert (or are handed fixtures
//! directly).

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, UiDeviceSettingsView};

/// See the module doc.
#[derive(Clone, Copy)]
pub(crate) struct AccessUi {
    pub on_access: Callback<AccessCommand>,
    pub device_settings: Signal<UiDeviceSettingsView>,
}

/// The app's access wiring, when there is an app (not in a story).
pub(crate) fn use_access_ui() -> Option<AccessUi> {
    try_consume_context::<AccessUi>()
}

/// The access callback, or an inert one (stories).
pub(crate) fn access_handler() -> EventHandler<AccessCommand> {
    match use_access_ui() {
        Some(ui) => EventHandler::new(move |command| ui.on_access.call(command)),
        None => EventHandler::new(|_| {}),
    }
}

use dioxus::prelude::*;

use crate::components::device_panel::DevicePanel;
use crate::stories::story::StoryDescriptor;
use crate::stories::story_fixtures::{
    studio_state_access_canceled, studio_state_blank_device_flash_offer, studio_state_connected,
    studio_state_connecting, studio_state_connection_lost, studio_state_deploying_project,
    studio_state_firmware_artifact_missing, studio_state_flash_confirm, studio_state_flash_failed,
    studio_state_flashing, studio_state_hardware_denied, studio_state_hardware_granted,
    studio_state_hardware_unsupported, studio_state_idle, studio_state_long_content,
    studio_state_multiple_project_selection_required, studio_state_post_flash_ready,
    studio_state_post_flash_reconnect_failed, studio_state_post_flash_reconnecting,
    studio_state_probing_server, studio_state_project_selection_required,
    studio_state_protocol_diagnostic, studio_state_provider_catalog,
    studio_state_reading_project_state, studio_state_ready, studio_state_recovery_required,
    studio_state_requesting_access,
};

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "device/idle",
        "DevicePanel",
        "Idle",
        "No endpoint has been discovered.",
    ),
    StoryDescriptor::new(
        "device/starting",
        "DevicePanel",
        "Starting",
        "Endpoint discovery has started the local worker.",
    ),
    StoryDescriptor::new(
        "device/connected",
        "DevicePanel",
        "Connected",
        "A browser-worker session is connected.",
    ),
    StoryDescriptor::new(
        "device/hardware-unsupported",
        "DevicePanel",
        "Hardware Unsupported",
        "Web Serial is unavailable.",
    ),
    StoryDescriptor::new(
        "device/hardware-denied",
        "DevicePanel",
        "Hardware Denied",
        "Serial permission was denied.",
    ),
    StoryDescriptor::new(
        "device/hardware-granted",
        "DevicePanel",
        "Hardware Granted",
        "A browser serial endpoint was granted.",
    ),
    StoryDescriptor::new(
        "device/long-session",
        "DevicePanel",
        "Long Session",
        "Long session identifiers should wrap cleanly.",
    ),
    StoryDescriptor::new(
        "device/protocol-diagnostic",
        "DevicePanel",
        "Protocol Diagnostic",
        "Malformed protocol diagnostics remain readable.",
    ),
    StoryDescriptor::new(
        "flow/provider-catalog",
        "ProvisioningFlow",
        "Provider Catalog",
        "Simulator and hardware providers are available for selection.",
    ),
    StoryDescriptor::new(
        "flow/requesting-access",
        "ProvisioningFlow",
        "Requesting Access",
        "The selected provider is waiting for browser/device access.",
    ),
    StoryDescriptor::new(
        "flow/access-canceled",
        "ProvisioningFlow",
        "Access Canceled",
        "The device chooser was canceled.",
    ),
    StoryDescriptor::new(
        "flow/opening-link",
        "ProvisioningFlow",
        "Opening Link",
        "Studio is opening the selected endpoint.",
    ),
    StoryDescriptor::new(
        "flow/probing-server",
        "ProvisioningFlow",
        "Probing Server",
        "Studio is identifying the connected target.",
    ),
    StoryDescriptor::new(
        "flow/blank-device-flash-offer",
        "ProvisioningFlow",
        "Blank Device",
        "A connected target needs LightPlayer firmware.",
    ),
    StoryDescriptor::new(
        "flow/flash-confirm",
        "ProvisioningFlow",
        "Flash Confirm",
        "A destructive firmware flash is waiting for confirmation.",
    ),
    StoryDescriptor::new(
        "flow/flashing",
        "ProvisioningFlow",
        "Flashing",
        "Firmware flashing progress is visible.",
    ),
    StoryDescriptor::new(
        "flow/firmware-artifact-missing",
        "ProvisioningFlow",
        "Firmware Missing",
        "The selected Studio build does not include a firmware artifact.",
    ),
    StoryDescriptor::new(
        "flow/flash-failed",
        "ProvisioningFlow",
        "Flash Failed",
        "Firmware flashing failed and recovery actions are visible.",
    ),
    StoryDescriptor::new(
        "flow/post-flash-reconnecting",
        "ProvisioningFlow",
        "Post-Flash Reconnect",
        "Firmware was flashed and Studio is reopening the server.",
    ),
    StoryDescriptor::new(
        "flow/post-flash-reconnect-failed",
        "ProvisioningFlow",
        "Reconnect Failed",
        "Firmware was flashed but the device did not reconnect.",
    ),
    StoryDescriptor::new(
        "flow/post-flash-ready",
        "ProvisioningFlow",
        "Post-Flash Ready",
        "Firmware was flashed and the server project state was attached.",
    ),
    StoryDescriptor::new(
        "flow/server-ready",
        "ProvisioningFlow",
        "Server Ready",
        "A server session is connected.",
    ),
    StoryDescriptor::new(
        "flow/reading-project-state",
        "ProvisioningFlow",
        "Reading Project State",
        "Studio is inspecting the connected server project state.",
    ),
    StoryDescriptor::new(
        "flow/project-selection-required",
        "ProvisioningFlow",
        "Project Selection",
        "No project is loaded and user intent is required.",
    ),
    StoryDescriptor::new(
        "flow/multiple-projects",
        "ProvisioningFlow",
        "Multiple Projects",
        "The server has more than one loaded project.",
    ),
    StoryDescriptor::new(
        "flow/recovery-required",
        "ProvisioningFlow",
        "Recovery Required",
        "The server reports a recovery-oriented state.",
    ),
    StoryDescriptor::new(
        "flow/deploying-project",
        "ProvisioningFlow",
        "Deploying Project",
        "The starter project is being written and loaded.",
    ),
    StoryDescriptor::new(
        "flow/ready",
        "ProvisioningFlow",
        "Ready",
        "A project is attached and inventory has been read.",
    ),
    StoryDescriptor::new(
        "flow/connection-lost",
        "ProvisioningFlow",
        "Connection Lost",
        "The device connection has degraded.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "device/idle" => Some(device_story(studio_state_idle(), false)),
        "device/starting" => Some(device_story(studio_state_connecting(), true)),
        "device/connected" => Some(device_story(studio_state_connected(), false)),
        "device/hardware-unsupported" => {
            Some(device_story(studio_state_hardware_unsupported(), false))
        }
        "device/hardware-denied" => Some(device_story(studio_state_hardware_denied(), false)),
        "device/hardware-granted" => Some(device_story(studio_state_hardware_granted(), false)),
        "device/long-session" => Some(device_story(studio_state_long_content(), false)),
        "device/protocol-diagnostic" => {
            Some(device_story(studio_state_protocol_diagnostic(), false))
        }
        "flow/provider-catalog" => Some(device_story(studio_state_provider_catalog(), false)),
        "flow/requesting-access" => Some(device_story(studio_state_requesting_access(), true)),
        "flow/access-canceled" => Some(device_story(studio_state_access_canceled(), false)),
        "flow/opening-link" => Some(device_story(studio_state_connecting(), true)),
        "flow/probing-server" => Some(device_story(studio_state_probing_server(), true)),
        "flow/blank-device-flash-offer" => {
            Some(device_story(studio_state_blank_device_flash_offer(), false))
        }
        "flow/flash-confirm" => Some(device_story(studio_state_flash_confirm(), false)),
        "flow/flashing" => Some(device_story(studio_state_flashing(), true)),
        "flow/firmware-artifact-missing" => Some(device_story(
            studio_state_firmware_artifact_missing(),
            false,
        )),
        "flow/flash-failed" => Some(device_story(studio_state_flash_failed(), false)),
        "flow/post-flash-reconnecting" => {
            Some(device_story(studio_state_post_flash_reconnecting(), true))
        }
        "flow/post-flash-reconnect-failed" => Some(device_story(
            studio_state_post_flash_reconnect_failed(),
            false,
        )),
        "flow/post-flash-ready" => Some(device_story(studio_state_post_flash_ready(), false)),
        "flow/server-ready" => Some(device_story(studio_state_connected(), false)),
        "flow/reading-project-state" => {
            Some(device_story(studio_state_reading_project_state(), true))
        }
        "flow/project-selection-required" => Some(device_story(
            studio_state_project_selection_required(),
            false,
        )),
        "flow/multiple-projects" => Some(device_story(
            studio_state_multiple_project_selection_required(),
            false,
        )),
        "flow/recovery-required" => Some(device_story(studio_state_recovery_required(), false)),
        "flow/deploying-project" => Some(device_story(studio_state_deploying_project(), true)),
        "flow/ready" => Some(device_story(studio_state_ready(), false)),
        "flow/connection-lost" => Some(device_story(studio_state_connection_lost(), false)),
        _ => None,
    }
}

fn device_story(state: lp_studio_core::StudioState, running: bool) -> Element {
    rsx! {
        DevicePanel {
            state,
            running,
            on_refresh_catalog: move |_| {},
            on_start_provider: move |_| {},
            on_confirm_firmware_flash: move |_| {},
            on_load_starter_project: move |_| {},
        }
    }
}

//! The editor's DEVICE pane (D23): hardware only, never the simulator.
//!
//! The pre-M5 four-step connect wizard (Select connection / Connect
//! device / Connect LightPlayer / Open project, with Provider/Endpoint/
//! Session rows) is gone — the simulator is the ever-present fallback
//! runtime a project simply runs in, and "device" means actual hardware
//! (D22). What remains is a pane about the hardware:
//!
//! - **Disconnected** (or the runtime is the sim): where this project
//!   usually lives (registry association), the ambient runtime line
//!   ("Running in the simulator"), and the door to the deploy dialog.
//! - **Connected**: the device's identity and its contents related to
//!   the library (connect-as-pull, D8), push/dialog actions, and a
//!   visually separate firmware section (flash / erase — D15).
//!
//! Connect/endpoint flows live inside the deploy dialog (M5); this pane
//! never renders provider plumbing.

use crate::app::device::DeployOp;
use crate::core::view::steps_view::{UiStepState, UiStepView};
use crate::{
    Controller, ControllerId, DeviceOp, DeviceSnapshot, LinkController, LinkState,
    ServerController, ServerFailureKind, ServerState, UiAction, UiMetric, UiPaneView, UiStatus,
    UiStepsView, UiViewContent,
};

use crate::app::places::{DeviceContent, DeviceSyncState};

pub struct DeviceController {
    pub(crate) link: LinkController,
    pub(crate) server: ServerController,
}

impl DeviceController {
    pub const NODE_ID: &'static str = "studio|device";
    /// The pane's single device section — also the activity target the
    /// connect/flash/push flows report progress against.
    pub const SECTION_DEVICE: &'static str = "device";
    /// Firmware operations, visually separate from project deploy (D15).
    pub const SECTION_FIRMWARE: &'static str = "firmware";

    pub fn new() -> Self {
        Self {
            link: LinkController::new(),
            server: ServerController::new(),
        }
    }

    pub fn snapshot(&self) -> DeviceSnapshot {
        DeviceSnapshot::new(self.link.snapshot(), self.server.snapshot())
    }

    pub fn is_lightplayer_connected(&self) -> bool {
        self.server.is_connected()
    }

    pub fn has_lightplayer_state(&self) -> bool {
        matches!(self.server.snapshot().state, ServerState::Connected { .. })
    }

    pub fn needs_firmware(&self) -> bool {
        matches!(
            self.server.snapshot().state,
            ServerState::Failed {
                kind: ServerFailureKind::NoFirmware,
                ..
            }
        )
    }

    /// Whether the active link is real hardware (the sim is not a
    /// device — D22).
    pub fn is_hardware_link(&self) -> bool {
        self.link
            .active_connection()
            .map(|connection| {
                !matches!(
                    connection.kind,
                    lpa_link::LinkConnectionKind::BrowserWorker { .. }
                )
            })
            .unwrap_or(false)
    }

    /// The editor's device pane (D23). `device_sync` is the connect-time
    /// pull result; `usual_device` names where this project usually
    /// lives when nothing is connected.
    pub fn view(
        &self,
        device_sync: Option<&DeviceSyncState>,
        usual_device: Option<String>,
    ) -> UiPaneView {
        let sections = if self.is_hardware_link() {
            let mut sections = vec![self.connected_device_section(device_sync)];
            sections.push(self.firmware_section());
            sections
        } else {
            vec![self.disconnected_device_section(usual_device)]
        };
        UiPaneView::new(
            Self::NODE_ID,
            "Device",
            self.status(device_sync),
            UiViewContent::Stack(Box::new(UiStepsView::new(sections))),
            Vec::new(),
        )
    }

    fn status(&self, device_sync: Option<&DeviceSyncState>) -> UiStatus {
        if !self.is_hardware_link() {
            return UiStatus::neutral("No device");
        }
        match &self.server.snapshot().state {
            ServerState::Failed {
                kind: ServerFailureKind::NoFirmware,
                ..
            } => UiStatus::warning("Ready to flash"),
            ServerState::Failed { .. } => UiStatus::error("Needs attention"),
            ServerState::Connecting { .. } => UiStatus::working("Connecting"),
            ServerState::Connected { .. } => {
                match device_sync.and_then(|sync| sync.identity.as_ref()) {
                    Some(identity) => UiStatus::good(identity.name.clone()),
                    None => UiStatus::good("Connected"),
                }
            }
            ServerState::Disconnected => UiStatus::working("Connecting"),
        }
    }

    /// The pane when no hardware is attached: association line, ambient
    /// runtime line, and the dialog entry.
    fn disconnected_device_section(&self, usual_device: Option<String>) -> UiStepView {
        let mut lines = Vec::new();
        if let Some(usual) = usual_device {
            lines.push(usual);
        }
        if matches!(self.link.state(), LinkState::Connected { .. })
            && !self.is_hardware_link()
            && self.has_lightplayer_state()
        {
            // D16: name where you are — ambient, not a "device"
            lines.push("Running in the simulator.".to_string());
        }
        if lines.is_empty() {
            lines.push("No device connected.".to_string());
        }
        UiStepView::new(Self::SECTION_DEVICE, "Device", UiStepState::Pending)
            .with_body(UiViewContent::text(lines.join("\n")))
            .with_actions(vec![UiAction::from_op(
                ControllerId::new(crate::app::device::DEPLOY_NODE_ID),
                DeployOp::OpenDialog { target_key: None },
            )])
    }

    /// The pane when hardware is attached: identity, contents relation,
    /// and the push/disconnect actions.
    fn connected_device_section(&self, device_sync: Option<&DeviceSyncState>) -> UiStepView {
        let state = match &self.server.snapshot().state {
            ServerState::Failed { .. } => UiStepState::NeedsAttention,
            ServerState::Connecting { .. } | ServerState::Disconnected => UiStepState::Active,
            ServerState::Connected { .. } => UiStepState::Complete,
        };
        let mut metrics = Vec::new();
        if let Some(sync) = device_sync {
            if let Some(identity) = &sync.identity {
                metrics.push(UiMetric::new("Name", &identity.name));
            }
            metrics.push(UiMetric::new("Holds", content_line(&sync.content)));
        }
        if let ServerState::Connected { protocol } = &self.server.snapshot().state {
            metrics.push(UiMetric::new("Protocol", protocol));
        }
        let body = if metrics.is_empty() {
            match &self.server.snapshot().state {
                ServerState::Failed {
                    kind: ServerFailureKind::NoFirmware,
                    ..
                } => UiViewContent::text("No LightPlayer firmware is running on this device."),
                ServerState::Failed { issue, .. } => UiViewContent::Issue(issue.clone()),
                _ => UiViewContent::text("Connecting to the device…"),
            }
        } else {
            UiViewContent::Metrics(metrics)
        };
        UiStepView::new(Self::SECTION_DEVICE, "Device", state)
            .with_body(body)
            .with_actions(self.connected_device_actions())
    }

    fn connected_device_actions(&self) -> Vec<UiAction> {
        vec![
            UiAction::from_op(
                ControllerId::new(crate::app::device::DEPLOY_NODE_ID),
                DeployOp::OpenDialog { target_key: None },
            )
            .with_label("Push to device…")
            .with_summary("Review and push a project to this device.")
            .with_icon("upload"),
            UiAction::from_op(self.node_id(), DeviceOp::DisconnectDevice)
                .with_label("Disconnect")
                .with_summary("Close the device session."),
        ]
    }

    /// Firmware ops, visually separate from deploy (D15).
    fn firmware_section(&self) -> UiStepView {
        UiStepView::new(Self::SECTION_FIRMWARE, "Firmware", UiStepState::Complete)
            .with_body(UiViewContent::text(
                "Firmware operations are separate from project deploys.",
            ))
            .with_actions(vec![
                UiAction::from_op(self.node_id(), DeviceOp::ProvisionFirmware)
                    .with_label("Update firmware")
                    .with_summary("Flash the bundled LightPlayer firmware.")
                    .with_icon("zap"),
                UiAction::from_op(self.node_id(), DeviceOp::ResetToBlank)
                    .with_label("Erase device")
                    .with_summary("Erase the device's flash storage entirely.")
                    .with_icon("remove"),
            ])
    }
}

/// One line for what the device holds, from the connect-time pull.
fn content_line(content: &DeviceContent) -> String {
    match content {
        DeviceContent::Empty => "Nothing yet".to_string(),
        DeviceContent::Known { slug, relation, .. } => match relation {
            lpc_history::SyncRelation::AtHead => format!("{slug} — up to date"),
            lpc_history::SyncRelation::Behind => format!("{slug} — behind your copy"),
            lpc_history::SyncRelation::Diverged => format!("{slug} — edited elsewhere"),
        },
        DeviceContent::Adopted { slug, .. } => format!("{slug} — pulled into your library"),
        DeviceContent::PendingIdentity { .. } => {
            "A project — name this device to keep it".to_string()
        }
        DeviceContent::Unreadable { .. } => "Contents unreadable".to_string(),
    }
}

impl Controller for DeviceController {
    type Op = DeviceOp;

    fn node_id(&self) -> ControllerId {
        ControllerId::new(Self::NODE_ID)
    }
}

impl Default for DeviceController {
    fn default() -> Self {
        Self::new()
    }
}

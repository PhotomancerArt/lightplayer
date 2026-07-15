//! Stories for the D23 device pane (M5): hardware only, never the sim.
//!
//! The pre-M5 wizard stories (provider/endpoint steps, provisioning
//! sequences over the step stack) died with the wizard; the deploy
//! dialog's stories cover those flows now.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DEPLOY_NODE_ID, DeployOp, DeviceController, DeviceOp, UiAction, UiMetric,
    UiPaneView, UiStatus, UiStepState, UiStepView, UiStepsView, UiViewContent,
};
use lpa_studio_web_story_macros::story;

use crate::core::PaneView;

fn pane(status: UiStatus, sections: Vec<UiStepView>) -> UiPaneView {
    UiPaneView::new(
        DeviceController::NODE_ID,
        "Device",
        status,
        UiViewContent::Stack(Box::new(UiStepsView::new(sections))),
        Vec::new(),
    )
}

fn story_pane(view: UiPaneView) -> Element {
    rsx! {
        section { class: "tw:max-w-[420px] tw:p-4",
            PaneView {
                view,
                primary: false,
                running: false,
                on_action: move |_| {},
            }
        }
    }
}

fn firmware_section() -> UiStepView {
    UiStepView::new(
        DeviceController::SECTION_FIRMWARE,
        "Firmware",
        UiStepState::Complete,
    )
    .with_body(UiViewContent::text(
        "Firmware operations are separate from project deploys.",
    ))
    .with_actions(vec![
        UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ProvisionFirmware,
        )
        .with_label("Update firmware"),
        UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ResetToBlank,
        )
        .with_label("Erase device"),
    ])
}

#[story]
pub(crate) fn disconnected_with_association() -> Element {
    story_pane(pane(
        UiStatus::neutral("No device"),
        vec![
            UiStepView::new(
                DeviceController::SECTION_DEVICE,
                "Device",
                UiStepState::Pending,
            )
            .with_body(UiViewContent::text(
                "Usually on Luna's porch sign.\nRunning in the simulator.",
            ))
            .with_actions(vec![UiAction::from_op(
                ControllerId::new(DEPLOY_NODE_ID),
                DeployOp::OpenDialog { target_key: None },
            )]),
        ],
    ))
}

#[story]
pub(crate) fn connected_at_head() -> Element {
    story_pane(pane(
        UiStatus::good("Luna's porch sign"),
        vec![
            UiStepView::new(
                DeviceController::SECTION_DEVICE,
                "Device",
                UiStepState::Complete,
            )
            .with_body(UiViewContent::Metrics(vec![
                UiMetric::new("Name", "Luna's porch sign"),
                UiMetric::new("Holds", "2026-07-02-0930-porch-sign — up to date"),
                UiMetric::new("Protocol", "fw-serial-v1"),
            ]))
            .with_actions(vec![
                UiAction::from_op(
                    ControllerId::new(DEPLOY_NODE_ID),
                    DeployOp::OpenDialog { target_key: None },
                )
                .with_label("Push to device…"),
                UiAction::from_op(
                    ControllerId::new(DeviceController::NODE_ID),
                    DeviceOp::DisconnectDevice,
                )
                .with_label("Disconnect"),
            ]),
            firmware_section(),
        ],
    ))
}

#[story]
pub(crate) fn ready_to_flash() -> Element {
    story_pane(pane(
        UiStatus::warning("Ready to flash"),
        vec![
            UiStepView::new(
                DeviceController::SECTION_DEVICE,
                "Device",
                UiStepState::Active,
            )
            .with_body(UiViewContent::text(
                "No LightPlayer firmware is running on this device.",
            ))
            .with_actions(vec![UiAction::from_op(
                ControllerId::new(DEPLOY_NODE_ID),
                DeployOp::OpenDialog { target_key: None },
            )]),
            firmware_section(),
        ],
    ))
}

/// Incompatible firmware (M4 hello gate): the pane outranks the server
/// state with "Reflash needed" and explains the incompatibility as an
/// issue; reflashing is the ONE affordance (firmware section).
#[story]
pub(crate) fn reflash_needed() -> Element {
    story_pane(pane(
        UiStatus::warning("Reflash needed"),
        vec![
            UiStepView::new(
                DeviceController::SECTION_DEVICE,
                "Device",
                UiStepState::NeedsAttention,
            )
            .with_body(UiViewContent::Issue(lpa_studio_core::UiIssue::new(
                "device firmware started its server loop but predates the wire hello; \
                 reflash the firmware to a compatible build",
            )))
            .with_actions(connected_actions()),
            firmware_section(),
        ],
    ))
}

/// Unresponsive device (readiness deadline expired without a diagnosis):
/// the server attach failed, so the pane needs attention and carries the
/// boot diagnosis; recovery stays reachable through the firmware section.
#[story]
pub(crate) fn unresponsive() -> Element {
    story_pane(pane(
        UiStatus::error("Needs attention"),
        vec![
            UiStepView::new(
                DeviceController::SECTION_DEVICE,
                "Device",
                UiStepState::NeedsAttention,
            )
            .with_body(UiViewContent::Issue(lpa_studio_core::UiIssue::new(
                "timed out waiting for device readiness; no serial output was received \
                 from the device",
            )))
            .with_actions(connected_actions()),
            firmware_section(),
        ],
    ))
}

fn connected_actions() -> Vec<UiAction> {
    vec![
        UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::OpenDialog { target_key: None },
        )
        .with_label("Push to device…"),
        UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::DisconnectDevice,
        )
        .with_label("Disconnect"),
    ]
}

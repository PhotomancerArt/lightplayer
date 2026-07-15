//! Deploy dialog stories: one per state (M5).

use dioxus::prelude::*;
use lpa_studio_core::app::places::{DeviceContent, DeviceIdentity};
use lpa_studio_core::{
    ContentHash, DeployState, DeployTarget, SyncRelation, UiDeployChoice, UiDeployView,
};
use lpa_studio_web_story_macros::story;

use crate::app::deploy::DeployDialog;

fn identity() -> DeviceIdentity {
    DeviceIdentity {
        uid: "dev_7pQr5St89uVwXy2C".to_string(),
        name: "Luna's porch sign".to_string(),
    }
}

fn target() -> DeployTarget {
    DeployTarget {
        project_uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
        slug: "2026-07-02-0930-porch-sign".to_string(),
        head: ContentHash::of(b"v7"),
        version_number: Some(7),
    }
}

fn choices() -> Vec<UiDeployChoice> {
    vec![
        UiDeployChoice {
            uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
        },
        UiDeployChoice {
            uid: "prj_9sLm2Xc44dQnUv7B".to_string(),
            slug: "2026-07-04-1102-basic".to_string(),
        },
    ]
}

fn dialog(state: DeployState) -> Element {
    let deploy = UiDeployView {
        state,
        choices: choices(),
        connect_actions: Vec::new(),
    };
    rsx! {
        section { class: "tw:relative tw:min-h-[420px] tw:p-4",
            DeployDialog { deploy, on_action: |_| {} }
        }
    }
}

#[story]
fn needs_device() -> Element {
    dialog(DeployState::NeedsDevice)
}

/// `NeedsDevice` with the catalog-derived hardware connect actions (the
/// shape `deploy_connect_actions` emits: connect + recovery open). Props
/// only — stories never touch `navigator.serial`.
#[story]
fn needs_device_with_connect_actions() -> Element {
    use lpa_studio_core::{
        ActionPriority, ControllerId, DeviceController, DeviceOp, LinkProviderKind, UiAction,
    };
    let device_node = ControllerId::new(DeviceController::NODE_ID);
    let deploy = UiDeployView {
        state: DeployState::NeedsDevice,
        choices: choices(),
        connect_actions: vec![
            UiAction::from_op(
                device_node.clone(),
                DeviceOp::OpenProvider {
                    provider_id: LinkProviderKind::Fake,
                },
            )
            .with_label("Connect ESP32")
            .with_summary("Connect an ESP32 over USB.")
            .with_icon("usb")
            .with_priority(ActionPriority::Primary),
            UiAction::from_op(
                device_node,
                DeviceOp::OpenProviderForRecovery {
                    provider_id: LinkProviderKind::Fake,
                },
            )
            .with_label("Open for flashing")
            .with_summary("Open the ESP32 connection without attaching LightPlayer.")
            .with_icon("usb")
            .with_priority(ActionPriority::Secondary),
        ],
    };
    rsx! {
        section { class: "tw:relative tw:min-h-[420px] tw:p-4",
            DeployDialog { deploy, on_action: |_| {} }
        }
    }
}

#[story]
fn blank_device() -> Element {
    dialog(DeployState::Blank {
        flashed_once: false,
    })
}

/// The Blank state's second rendering: an Incompatible/Unresponsive device
/// after one reflash still isn't answering — the dialog keeps flashing as
/// the way forward with the "still no answer" copy.
#[story]
fn blank_after_reflash() -> Element {
    dialog(DeployState::Blank { flashed_once: true })
}

#[story]
fn needs_identity() -> Element {
    dialog(DeployState::NeedsIdentity {
        suggested_name: String::new(),
    })
}

#[story]
fn choosing_package() -> Element {
    dialog(DeployState::ChoosingPackage { device: identity() })
}

#[story]
fn reviewing_behind() -> Element {
    dialog(DeployState::Reviewing {
        device: identity(),
        target: target(),
        on_device: DeviceContent::Known {
            project_uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            observed: ContentHash::of(b"v5"),
            relation: SyncRelation::Behind,
        },
    })
}

#[story]
fn reviewing_diverged() -> Element {
    dialog(DeployState::Reviewing {
        device: identity(),
        target: target(),
        on_device: DeviceContent::Known {
            project_uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            observed: ContentHash::of(b"foreign"),
            relation: SyncRelation::Diverged,
        },
    })
}

#[story]
fn pushing() -> Element {
    dialog(DeployState::Pushing {
        device: identity(),
        target: target(),
    })
}

#[story]
fn done() -> Element {
    dialog(DeployState::Done {
        device: identity(),
        pushed: target(),
    })
}

#[story]
fn failed() -> Element {
    dialog(DeployState::Failed {
        message: "Push failed: the serial connection dropped mid-write.".to_string(),
        resume: Box::new(DeployState::Reviewing {
            device: identity(),
            target: target(),
            on_device: DeviceContent::Empty,
        }),
    })
}

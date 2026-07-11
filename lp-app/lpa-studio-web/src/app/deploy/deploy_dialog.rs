//! The deploy dialog overlay.
//!
//! P3 ships a functional skeleton (state title + copy + the state's
//! actions through the shared action system); P4 gives each state its
//! real layout. The dialog renders over whatever the shell shows.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DEPLOY_NODE_ID, DeployOp, DeployState, UiAction, UiDeployView,
};

use crate::core::{ActionButton, ActionButtonVariant, quiet_action_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DeployDialog(deploy: UiDeployView, on_action: EventHandler<UiAction>) -> Element {
    let close = UiAction::from_op(ControllerId::new(DEPLOY_NODE_ID), DeployOp::CloseDialog);
    let busy = matches!(
        deploy.state,
        DeployState::Flashing | DeployState::Stamping { .. } | DeployState::Pushing { .. }
    );

    rsx! {
        div {
            class: "tw:fixed tw:inset-0 tw:z-50 tw:flex tw:items-center tw:justify-center tw:bg-black/60",
            onclick: move |_| {
                if !busy {
                    on_action.call(close.clone());
                }
            },
            section {
                class: "tw:w-[min(560px,92vw)] tw:rounded-lg tw:border tw:border-border tw:bg-card tw:p-5 tw:shadow-xl",
                onclick: move |event| event.stop_propagation(),
                header { class: "tw:mb-3 tw:flex tw:items-center tw:justify-between",
                    h2 { class: "tw:m-0 tw:text-base tw:font-semibold tw:text-strong-foreground",
                        {dialog_title(&deploy.state)}
                    }
                    if !busy {
                        button {
                            class: quiet_action_class(),
                            r#type: "button",
                            onclick: {
                                let close = UiAction::from_op(
                                    ControllerId::new(DEPLOY_NODE_ID),
                                    DeployOp::CloseDialog,
                                );
                                move |_| on_action.call(close.clone())
                            },
                            "Close"
                        }
                    }
                }
                DeployDialogBody { deploy: deploy.clone(), on_action }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DeployDialogBody(deploy: UiDeployView, on_action: EventHandler<UiAction>) -> Element {
    let deploy_action = |op: DeployOp| UiAction::from_op(ControllerId::new(DEPLOY_NODE_ID), op);
    match &deploy.state {
        DeployState::NeedsDevice => rsx! {
            p { class: "tw:mb-3 tw:text-sm tw:text-muted-foreground",
                "Connect an ESP32 over USB to get started."
            }
            div { class: "tw:flex tw:flex-wrap tw:gap-2",
                for action in deploy.connect_actions.clone() {
                    ActionButton {
                        action,
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }
        },
        DeployState::Blank { flashed_once } => rsx! {
            p { class: "tw:mb-3 tw:text-sm tw:text-muted-foreground",
                if *flashed_once {
                    "Still no answer from LightPlayer — flashing again sometimes helps."
                } else {
                    "This device has no LightPlayer firmware yet. Install it to continue."
                }
            }
            ActionButton {
                action: deploy_action(DeployOp::FlashFirmware),
                running: false,
                variant: ActionButtonVariant::Solid,
                on_action,
            }
        },
        DeployState::NeedsIdentity { suggested_name } => rsx! {
            NameForm { suggested: suggested_name.clone(), on_action }
        },
        DeployState::ChoosingPackage { device } => rsx! {
            p { class: "tw:mb-3 tw:text-sm tw:text-muted-foreground",
                "Choose a project to push to {device.name}."
            }
            div { class: "tw:grid tw:gap-1",
                for choice in deploy.choices.clone() {
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        onclick: {
                            let key = choice.uid.clone();
                            move |_| {
                                on_action.call(UiAction::from_op(
                                    ControllerId::new(DEPLOY_NODE_ID),
                                    DeployOp::ChoosePackage { key: key.clone() },
                                ))
                            }
                        },
                        "{choice.slug}"
                    }
                }
            }
        },
        DeployState::Reviewing {
            device,
            target,
            on_device,
        } => rsx! {
            p { class: "tw:mb-1 tw:text-sm tw:text-strong-foreground",
                "Push {target.slug}"
                if let Some(version) = target.version_number {
                    " (v{version})"
                }
                " to {device.name}."
            }
            p { class: "tw:mb-3 tw:text-xs tw:text-muted-foreground",
                {on_device_line(on_device)}
            }
            div { class: "tw:flex tw:flex-wrap tw:gap-2",
                ActionButton {
                    action: deploy_action(DeployOp::ConfirmPush),
                    running: false,
                    variant: ActionButtonVariant::Solid,
                    on_action,
                }
                if diverged(on_device) {
                    ActionButton {
                        action: deploy_action(DeployOp::AdoptDeviceCopy),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                    ActionButton {
                        action: deploy_action(DeployOp::KeepBothFork),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }
        },
        DeployState::Inspecting => progress_line("Checking what's on the device…"),
        DeployState::Flashing => progress_line("Flashing firmware…"),
        DeployState::Stamping { name } => progress_line(&format!("Naming this device \"{name}\"…")),
        DeployState::Pushing { target, .. } => progress_line(&format!("Pushing {}…", target.slug)),
        DeployState::Done { device, pushed } => rsx! {
            p { class: "tw:mb-3 tw:text-sm tw:text-strong-foreground",
                "{pushed.slug} is running on {device.name}."
            }
            p { class: "tw:text-xs tw:text-muted-foreground",
                "Swap in another board to push it there too."
            }
        },
        DeployState::Failed { message, .. } => rsx! {
            p { class: "tw:mb-3 tw:text-sm tw:text-status-error-foreground", "{message}" }
            ActionButton {
                action: deploy_action(DeployOp::RetryFailed),
                running: false,
                variant: ActionButtonVariant::Solid,
                on_action,
            }
        },
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NameForm(suggested: String, on_action: EventHandler<UiAction>) -> Element {
    let mut name = use_signal(|| suggested.clone());
    let empty = name.read().trim().is_empty();
    rsx! {
        p { class: "tw:mb-3 tw:text-sm tw:text-muted-foreground",
            "Give this device a real name — it's how you'll recognize it later."
        }
        form {
            class: "tw:flex tw:gap-2",
            onsubmit: move |event| {
                event.prevent_default();
                let value = name.read().trim().to_string();
                if !value.is_empty() {
                    on_action.call(UiAction::from_op(
                        ControllerId::new(DEPLOY_NODE_ID),
                        DeployOp::StampIdentity { name: value },
                    ));
                }
            },
            input {
                class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground",
                placeholder: "Luna's porch sign",
                value: "{name}",
                oninput: move |event| name.set(event.value()),
            }
            button {
                class: quiet_action_class(),
                r#type: "submit",
                disabled: empty,
                "Continue"
            }
        }
    }
}

fn dialog_title(state: &DeployState) -> &'static str {
    match state {
        DeployState::NeedsDevice => "Connect a device",
        DeployState::Blank { .. } => "Install firmware",
        DeployState::Inspecting => "Checking the device…",
        DeployState::NeedsIdentity { .. } => "Name this device",
        DeployState::ChoosingPackage { .. } => "Choose a project",
        DeployState::Reviewing { .. } => "Review push",
        DeployState::Flashing => "Flashing…",
        DeployState::Stamping { .. } => "Naming…",
        DeployState::Pushing { .. } => "Pushing…",
        DeployState::Done { .. } => "Done",
        DeployState::Failed { .. } => "Something went wrong",
    }
}

fn on_device_line(content: &lpa_studio_core::app::places::DeviceContent) -> String {
    use lpa_studio_core::app::places::DeviceContent;
    match content {
        DeviceContent::Empty => "The device is empty.".to_string(),
        DeviceContent::Known { slug, relation, .. } => match relation {
            lpa_studio_core::SyncRelation::AtHead => format!("Now running {slug} — up to date."),
            lpa_studio_core::SyncRelation::Behind => {
                format!("Now running {slug} — behind your copy.")
            }
            lpa_studio_core::SyncRelation::Diverged => format!(
                "Now running {slug}, edited elsewhere. Its current contents are already saved in your library."
            ),
        },
        DeviceContent::Adopted { slug, .. } => {
            format!("Now running {slug} — pulled into your library at connect.")
        }
        DeviceContent::PendingIdentity { .. } => "The device holds an unnamed project.".to_string(),
        DeviceContent::Unreadable { .. } => "The device's contents are unreadable.".to_string(),
    }
}

fn diverged(content: &lpa_studio_core::app::places::DeviceContent) -> bool {
    matches!(
        content,
        lpa_studio_core::app::places::DeviceContent::Known {
            relation: lpa_studio_core::SyncRelation::Diverged,
            ..
        }
    )
}

fn progress_line(text: &str) -> Element {
    rsx! {
        p { class: "tw:text-sm tw:text-status-working-foreground", "{text}" }
    }
}

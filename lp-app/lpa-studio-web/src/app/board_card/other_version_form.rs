//! "Other version…" inline in the firmware bar's details
//! ([`UiDetailPanel::OtherVersion`]): the install offer's parameters
//! ([`OfferParamsForm`]: the version choice and its find box), the copy of a
//! Lasting pick (core's title and sentence: what installing that version
//! changes, read before the two clicks rather than only on hover), the
//! press ([`OfferPressButton`], which arms a Lasting binding on itself), and
//! "From a file…" ([`FirmwareFileButton`]) for a custom build. No popover in
//! a popover: the details card is the one box, and this is a section of it.
//!
//! Today's card draws the same form in its own popover until it goes.
//!
//! [`UiDetailPanel::OtherVersion`]: lpa_studio_core::UiDetailPanel::OtherVersion

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceId, OfferArgs, PickedFirmwareFile, UiAction, UiOffer, firmware_file_action,
};

use super::restore_from_file_button::{open_file_picker, say};
use crate::app::agent::AgentMark;
use crate::core::{ActionButtonVariant, OfferParamsForm, OfferPressButton, pressed_or_refused};

/// Stories only: the form mounted with values already picked, and its press
/// armed when `armed`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OfferPickerPreview {
    pub args: OfferArgs,
    pub armed: bool,
}

/// The form. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OtherVersionForm(
    /// The board the files go to (From a file… reads them for it).
    device: DeviceId,
    /// `devices/<board>/install-firmware`, with its version choice.
    install: UiOffer,
    /// `devices/<board>/install-firmware-file`, when a custom build can be
    /// picked.
    #[props(default)]
    from_file: Option<UiOffer>,
    /// Stories only.
    #[props(default)]
    preview: Option<OfferPickerPreview>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let initial = preview
        .as_ref()
        .map(|preview| preview.args.clone())
        .unwrap_or_default();
    let args = use_signal(move || initial);
    let current = args.read().clone();
    let copy = pressed_or_refused(&install, &current)
        .meta()
        .consequence
        .copy()
        .cloned();
    let armed_preview = preview.as_ref().is_some_and(|preview| preview.armed);
    rsx! {
        div { class: FORM_CLASS,
            OfferParamsForm { offer: install.clone(), args }
            if let Some(copy) = copy {
                div { class: COPY_CLASS,
                    p { class: "tw:m-0 tw:font-semibold", "{copy.title}" }
                    p { class: "tw:m-0 tw:text-muted-foreground", "{copy.message}" }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-2",
                if let Some(file) = from_file {
                    AgentMark { path: file.path.clone(),
                        FirmwareFileButton { device, offer: file, on_action }
                    }
                } else {
                    span {}
                }
                AgentMark { path: install.path.clone(),
                    OfferPressButton {
                        offer: install,
                        args: current,
                        variant: ActionButtonVariant::Outline,
                        armed_preview,
                        on_action,
                    }
                }
            }
        }
    }
}

/// "From a file…" (`install-firmware-file`): a quiet button in the offer's
/// own words, paired with a hidden multi-file input — a file dialog cannot
/// be a [`UiAction`], the same reasoning as the restore-from-file button.
/// Core's offer exists so the app agent can see this is possible
/// (`needs_user_activation`); picking files reads their bytes and hands them
/// to core ([`firmware_file_action`]), which checks them against their
/// manifest and lists the build — or says why not.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn FirmwareFileButton(
    device: DeviceId,
    offer: UiOffer,
    on_action: EventHandler<UiAction>,
) -> Element {
    let input_id = format!("firmware-file-{}", device.0);
    let disabled = !offer.is_enabled();
    rsx! {
        button {
            class: FIRMWARE_FILE_BUTTON_CLASS,
            r#type: "button",
            disabled,
            title: "{offer.summary()}",
            onclick: {
                let input_id = input_id.clone();
                move |_| open_file_picker(&input_id)
            },
            "{offer.label()}"
        }
        input {
            class: "tw:hidden",
            id: "{input_id}",
            r#type: "file",
            multiple: true,
            accept: ".json,.bin,.z",
            onchange: move |event| {
                let files = event.files();
                spawn(async move {
                    let mut picked = Vec::new();
                    for file in files {
                        let name = file.name();
                        match file.read_bytes().await {
                            Ok(bytes) => picked.push(PickedFirmwareFile { name, bytes: bytes.to_vec() }),
                            Err(error) => {
                                log::warn!("from a file: could not read {name}: {error}");
                                say(&format!("{name} could not be read"));
                                return;
                            }
                        }
                    }
                    if !picked.is_empty() {
                        on_action.call(firmware_file_action(device, picked));
                    }
                });
            },
        }
    }
}

/// The form: the params, the copy, the press — no frame of its own.
const FORM_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-2.5";

/// What a Lasting pick changes, in core's words: the warning's ink, wrapping,
/// never a box inside the card.
const COPY_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-1 tw:text-[11px] tw:leading-snug tw:text-status-warning-foreground tw:whitespace-normal tw:break-words";

/// "From a file…": a text button, quieter than the press beside it.
const FIRMWARE_FILE_BUTTON_CLASS: &str = "tw:shrink-0 tw:whitespace-nowrap tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-0 tw:py-1.5 tw:text-[11px] tw:font-semibold tw:text-subtle-foreground tw:underline tw:decoration-dotted tw:underline-offset-2 tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60";

#[cfg(test)]
mod tests {
    use super::*;

    /// No box in a box: the form and the Lasting copy draw no frame of
    /// their own inside the details card.
    #[test]
    fn the_form_draws_no_frame_of_its_own() {
        for class in [FORM_CLASS, COPY_CLASS] {
            assert!(!class.contains("tw:border"), "{class}");
            assert!(!class.contains("rounded"), "{class}");
            assert!(!class.contains("tw:bg-"), "{class}");
        }
        assert!(COPY_CLASS.contains("tw:break-words"));
    }
}

//! The question an update asks before it moves a board's files to the new
//! layout (the C6 repartition), and the refusal when they do not fit.
//!
//! Core decides every word and every verb ([`UiLayoutPanel`] for the words,
//! the view's offers under `devices/<board>` for the verbs, which the card
//! resolves into [`LayoutSheetVerbs`]); this sheet only lays them out —
//! Download backup always, Continue and Cancel while the question is open.
//! Page-level like the Unlock sheet, so it rises over whatever the user is
//! looking at and never changes the card's height.
//!
//! The sheet IS the question, so its Continue acts on one press: the user
//! pressed Update (an armed press of its own) to get here, and the sheet
//! asks, in full, what Continue's own arm would ask again (G1 walk,
//! 2026-10-03, Yona: "the continue button on the dialog doesn't really need
//! a confirm … they already committed to it once"). Continue keeps its
//! Lasting level and tint, so the app agent still hands it to the user —
//! see `ActionButton`'s `asked_by_surface`.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiLayoutPanel};

use crate::core::{ActionButton, ActionButtonVariant, quiet_action_class};

/// The sheet's verbs, as the view offers them at the panel's paths. One
/// the tree does not hold is simply not drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutSheetVerbs {
    pub download: Option<UiAction>,
    pub cancel: Option<UiAction>,
    pub continue_action: Option<UiAction>,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceLayoutSheet(
    panel: UiLayoutPanel,
    verbs: LayoutSheetVerbs,
    /// The card's title, so the sheet says which board.
    device_name: String,
    on_action: EventHandler<UiAction>,
    /// A refusal has no Cancel (the activity already ended): this closes
    /// the sheet in the page. Presentation only — nothing in the model
    /// changes, and the card's outcome line still says what happened.
    #[props(default)]
    on_close: Option<EventHandler<()>>,
    /// Stories: a capture pins the sheet in its box instead of the viewport.
    #[props(default)]
    inline: bool,
) -> Element {
    let frame_class = if inline {
        INLINE_FRAME_CLASS
    } else {
        OVERLAY_CLASS
    };
    rsx! {
        div { class: frame_class,
            role: "dialog",
            aria_modal: "true",
            aria_label: "{panel.title}",
            div { class: SHEET_CLASS,
                div { class: "tw:mx-auto tw:-mt-1 tw:h-1 tw:w-9 tw:rounded-full tw:bg-border-strong tw:sm:hidden" }
                h2 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground", "{panel.title}" }
                p { class: "tw:m-0 tw:truncate tw:font-mono tw:text-xs tw:text-muted-foreground", "{device_name}" }
                p { class: "tw:m-0 tw:text-sm tw:leading-snug tw:text-strong-foreground", "{panel.body}" }
                if let Some(warning) = panel.warning.as_deref() {
                    p { class: "tw:m-0 tw:rounded-md tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2.5 tw:py-2 tw:text-sm tw:leading-snug tw:text-status-warning-foreground",
                        "{warning}"
                    }
                }
                div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:pt-1",
                    if let Some(download) = verbs.download.clone() {
                        ActionButton {
                            key: "{\"layout-download\"}",
                            action: download,
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                    span { class: "tw:min-w-0 tw:flex-1" }
                    if let Some(close) = on_close {
                        button {
                            class: quiet_action_class(),
                            r#type: "button",
                            onclick: move |_| close.call(()),
                            "Close"
                        }
                    }
                    if let Some(cancel) = verbs.cancel.clone() {
                        ActionButton {
                            key: "{\"layout-cancel\"}",
                            action: cancel,
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                    if let Some(next) = verbs.continue_action.clone() {
                        ActionButton {
                            key: "{\"layout-continue\"}",
                            action: next,
                            running: false,
                            variant: ActionButtonVariant::Outline,
                            asked_by_surface: CONTINUE_ASKED_BY_THE_SHEET,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// Hands the user a backup core prepared ("Download backup") as a `.zip`.
/// Core bumps `seq` per request; this downloads when it sees `seq` move
/// past the one it mounted with, so a page revisit never re-downloads.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BackupDownloadWatcher(download: Option<lpa_studio_core::BackupDownload>) -> Element {
    let mounted_seq = download.as_ref().map(|d| d.seq);
    let mut last_seq = use_signal(|| mounted_seq);
    use_effect(use_reactive!(|download| {
        let Some(download) = download else {
            return;
        };
        if last_seq.peek().is_some_and(|seen| seen >= download.seq) {
            return;
        }
        last_seq.set(Some(download.seq));
        #[cfg(target_arch = "wasm32")]
        if let Err(error) = super::package_export::trigger_zip_download(
            &download.file_name,
            download.bytes.as_slice(),
        ) {
            log::warn!("backup download failed: {error:?}");
        }
    }));
    rsx! {}
}

/// The sheet's title and body are the question Continue answers, so
/// Continue does not ask again (see the module doc).
const CONTINUE_ASKED_BY_THE_SHEET: bool = true;

/// The viewport overlay: a dim backdrop, the sheet at the bottom on a
/// phone and centred from `sm` up (the Unlock sheet's frame).
const OVERLAY_CLASS: &str = "tw:fixed tw:inset-0 tw:z-50 tw:flex tw:items-end tw:justify-center tw:bg-black/50 tw:sm:items-center tw:sm:p-6";

/// The stories' frame: the same sheet, in flow.
const INLINE_FRAME_CLASS: &str = "tw:flex tw:justify-center tw:bg-black/50 tw:p-3";

const SHEET_CLASS: &str = "tw:grid tw:w-full tw:max-w-md tw:gap-3 tw:rounded-t-xl tw:border tw:border-b-0 tw:border-border-strong tw:bg-card-raised tw:p-4 tw:pb-[calc(1.5rem+env(safe-area-inset-bottom,0px))] tw:shadow-2xl tw:sm:rounded-lg tw:sm:border-b tw:sm:pb-4";

//! The question an update asks before it moves a board's files to the new
//! layout (the C6 repartition), and the refusal when they do not fit — the
//! firmware bar's details now ([`UiDetailPanel::Layout`], DC22): core raises
//! those details while its question is open, so the question rises where
//! the board's firmware is, and it is answered there. No longer an overlay.
//!
//! Core decides every word ([`UiLayoutPanel`]) and every verb: the panel
//! names the paths of Download backup, Continue and Cancel, and each button
//! presses the offer the view's tree holds at its path
//! ([`OfferAction`]) — this panel builds no action. Download backup always,
//! Continue and Cancel while the question is open; a refusal's Close is the
//! page's own (nothing is running to cancel).
//!
//! The panel IS the question, so its Continue acts on one press: the user
//! pressed Update (an armed press of its own) to get here, and the panel
//! asks, in full, what Continue's own arm would ask again (G1 walk,
//! 2026-10-03, Yona: "the continue button on the dialog doesn't really need
//! a confirm … they already committed to it once"). Continue keeps its
//! Lasting level and tint, so the app agent still hands it to the user —
//! see `ActionButton`'s `asked_by_surface`.
//!
//! [`UiDetailPanel::Layout`]: lpa_studio_core::UiDetailPanel::Layout

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiLayoutPanel};

use crate::app::board_card::OfferAction;
use crate::core::{ActionButtonVariant, quiet_action_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceLayoutSheet(
    panel: UiLayoutPanel,
    on_action: EventHandler<UiAction>,
    /// A refusal has no Cancel (the activity already ended): this closes
    /// it in the page. Presentation only — nothing in the model changes,
    /// and the card's outcome still says what happened.
    #[props(default)]
    on_close: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        section { class: SECTION_CLASS, aria_label: "{panel.title}",
            h3 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "{panel.title}" }
            p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-strong-foreground", "{panel.body}" }
            if let Some(warning) = panel.warning.as_deref() {
                p { class: WARNING_CLASS, "{warning}" }
            }
            div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:pt-1",
                OfferAction {
                    key: "{\"layout-download\"}",
                    path: panel.download.clone(),
                    variant: ActionButtonVariant::Quiet,
                    on_action,
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
                if let Some(cancel) = panel.cancel.clone() {
                    OfferAction {
                        key: "{\"layout-cancel\"}",
                        path: cancel,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
                if let Some(next) = panel.continue_action.clone() {
                    OfferAction {
                        key: "{\"layout-continue\"}",
                        path: next,
                        variant: ActionButtonVariant::Outline,
                        asked_by_surface: CONTINUE_ASKED_BY_THE_PANEL,
                        on_action,
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

/// The panel's title and body are the question Continue answers, so
/// Continue does not ask again (see the module doc).
const CONTINUE_ASKED_BY_THE_PANEL: bool = true;

/// A section of the details card: its divider and padding, no frame of its
/// own (no box in a box).
const SECTION_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-2 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:px-3 tw:py-2.5 tw:first:border-t-0";

/// The warning: its family's ink, wrapping — not a box inside the card.
const WARNING_CLASS: &str =
    "tw:m-0 tw:text-xs tw:leading-snug tw:text-status-warning-foreground tw:break-words";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{OfferPath, UiOfferTree};

    use super::*;
    use crate::app::board_card::card_test_fixtures::{
        attribute_values, board, card_and_tree, porch_view, render,
    };
    use crate::core::OffersProvider;

    /// The panel builds no action: each button presses the offer the tree
    /// holds at the panel's path — the question's Continue among them — and
    /// a path the tree does not offer draws nothing.
    #[test]
    fn the_panel_presses_its_own_offer_paths() {
        // Any published verbs stand in for the layout's (their paths are
        // all the panel reads).
        let (_, tree) = card_and_tree(&porch_view());
        let question = UiLayoutPanel {
            title: "Move this board's files?".to_string(),
            body: "The new firmware lays its files out differently.".to_string(),
            warning: None,
            download: board().child("disconnect"),
            continue_action: Some(board().child("forget")),
            cancel: Some(board().child("reset-board")),
        };
        let html = render_panel(tree.clone(), question.clone());
        let marked = attribute_values(&html, "data-offer-path");
        for path in ["disconnect", "forget", "reset-board"] {
            let path = board().child(path).to_string();
            assert!(marked.contains(&path), "{path} unmarked: {marked:?}");
        }
        // Continue reads from the tree: gone from it, it is not drawn.
        let missing = UiLayoutPanel {
            continue_action: Some(OfferPath::parse("devices/new-7/continue-update").unwrap()),
            ..question
        };
        let html = render_panel(tree, missing);
        assert!(
            !attribute_values(&html, "data-offer-path")
                .contains(&"devices/new-7/continue-update".to_string()),
            "{html}"
        );
    }

    /// No box in a box: the panel is a section of the details card.
    #[test]
    fn the_panel_is_a_section_with_no_frame() {
        assert!(SECTION_CLASS.contains("tw:border-t"));
        assert!(!SECTION_CLASS.contains("rounded"));
        assert!(!SECTION_CLASS.contains("tw:bg-"));
        assert!(!WARNING_CLASS.contains("tw:border"));
        assert!(CONTINUE_ASKED_BY_THE_PANEL);
    }

    fn render_panel(tree: UiOfferTree, panel: UiLayoutPanel) -> String {
        render(PanelRoot, PanelRootProps { tree, panel })
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn PanelRoot(tree: UiOfferTree, panel: UiLayoutPanel) -> Element {
        rsx! {
            OffersProvider { offers: tree,
                DeviceLayoutSheet { panel, on_action: |_| {} }
            }
        }
    }
}

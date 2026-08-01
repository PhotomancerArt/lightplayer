//! **Play mode**: the root module's panel, and nothing else.
//!
//! `docs/design/panel.md` P12 — play mode renders panels only, never
//! faces: no preview drawers, no children, no wiring, no authoring
//! surfaces. It speaks the two runtime ops plus reads, and "anything play
//! mode can do, an end user is allowed to do".
//!
//! The surface is deliberately *not* a card: it fills the viewport, because
//! at this zoom level there is no workspace to sit inside. It carries the
//! project name, the module's own controls, and the nested groups
//! recursively (R8) — the same [`super::ModulePanel`] the face hosts,
//! rendered at play density.
//!
//! The hero preview is present but optional: on a phone at 4 a.m. the
//! controls matter more than a thumbnail, so the preview is a slim banner
//! rather than the face's dominant hero.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPanelGroup, UiProducedProduct};

use crate::app::node::ProductPreview;

use super::{ModulePanel, PanelGesture};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PlayModeSurface(
    /// The root module's panel — the end-user surface (R8).
    panel: UiPanelGroup,
    /// The root module's output mirror, as a slim banner.
    #[props(default = None)]
    preview: Option<UiProducedProduct>,
    /// Panel-state auto-save (P11): an end user owns this, so it stays
    /// reachable in play mode. `None` hides the switch.
    #[props(default = Some(true))]
    auto_save: Option<bool>,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let title = panel.label.clone();

    rsx! {
        div { class: "tw:grid tw:min-h-full tw:min-w-0 tw:content-start tw:gap-0 tw:bg-page",
            header { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2 tw:border-b tw:border-border-strong tw:px-4 tw:py-3",
                h1 { class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-lg tw:text-strong-foreground", "{title}" }
                span { class: "tw:ml-auto tw:flex-none tw:text-[0.6rem] tw:font-bold tw:uppercase tw:tracking-[0.14em] tw:text-dim-foreground",
                    "play"
                }
            }
            if let Some(preview) = preview {
                div { class: "tw:border-b tw:border-border-strong",
                    ProductPreview {
                        kind: preview.kind,
                        preview: preview.preview.clone(),
                        tracking: preview.tracking,
                        frame: preview.frame,
                        focus_action: None,
                        on_action,
                    }
                }
            }
            ModulePanel {
                panel,
                auto_save,
                play: true,
                on_panel,
                on_action,
            }
        }
    }
}

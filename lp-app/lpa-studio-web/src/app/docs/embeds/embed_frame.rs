//! The shared chrome every live docs embed sits in.
//!
//! One frame for all of them, so an article reads as a column of the same
//! object rather than five different boxes: a hairline border on the card
//! surface, an optional caption row, an optional quiet note on the right
//! (what the embed is doing — "Read-only", "Loading…"), and a reset chip
//! when the embed owns something resettable.
//!
//! Design language is Studio's card idiom (`border-border` hairline,
//! `rounded-md`, `bg-card`) rather than anything docs-specific: the point
//! of interactive docs is that a reader meets the real UI, so its frame
//! must not look like a screenshot mount.
//!
//! **Violet is reserved.** The bound/binding convention is app-wide; embed
//! chrome never borrows it for decoration. The reset chip is a
//! [`InlineButton`] in the revert (amber/warning) family, matching the
//! panel's own ↺.
//!
//! Height is reserved by the *caller* (a loading state is the same box as
//! the live one) so an article never lurches when a sim finishes booting.

use dioxus::prelude::*;

use crate::base::{InlineButton, InlineButtonTone, StudioIconName};

/// The frame around one live embed.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn EmbedFrame(
    /// Caption above the surface — what the reader is looking at.
    #[props(default)]
    caption: Option<String>,
    /// Quiet right-aligned status word ("Loading…", "Read-only").
    #[props(default)]
    note: Option<String>,
    /// The reset chip; absent means the embed owns nothing to reset.
    #[props(default)]
    on_reset: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let has_header = caption.is_some() || note.is_some() || on_reset.is_some();
    rsx! {
        div { class: "tw:mb-1.5 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:last:mb-0",
            if has_header {
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-b tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1",
                    if let Some(caption) = caption {
                        span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:text-muted-foreground",
                            "{caption}"
                        }
                    }
                    span { class: "tw:ml-auto tw:flex tw:flex-none tw:items-center tw:gap-1.5",
                        if let Some(note) = note {
                            span { class: "tw:text-[0.65rem] tw:uppercase tw:tracking-[0.1em] tw:text-dim-foreground",
                                "{note}"
                            }
                        }
                        if let Some(on_reset) = on_reset {
                            InlineButton {
                                label: "Reset".to_string(),
                                title: "Put this example back the way it started".to_string(),
                                icon: StudioIconName::Revert,
                                text: "Reset".to_string(),
                                tone: InlineButtonTone::Warning,
                                on_press: move |()| on_reset.call(()),
                            }
                        }
                    }
                }
            }
            {children}
        }
    }
}

/// The calm "this is still coming up" body: a reserved box with a quiet
/// line in it. `min_height` is the caller's reservation — a sim canvas
/// passes the height its live preview will occupy so nothing jumps.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn EmbedLoading(
    /// What is being waited for, in a reader's words.
    message: String,
    /// Reserved height in CSS pixels — the live surface's own height.
    #[props(default = 200)]
    min_height: u32,
) -> Element {
    rsx! {
        div {
            class: "tw:grid tw:place-items-center tw:bg-card-subtle tw:p-4 tw:text-center",
            style: "min-height: {min_height}px",
            span { class: "tw:text-xs tw:text-subtle-foreground", "{message}" }
        }
    }
}

/// The authoring-mistake body: an article named something that does not
/// exist (an unknown sim, an unregistered code figure). Loud on purpose —
/// the generated docs checks catch these before merge, so one reaching a
/// reader means a check has a hole.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn EmbedProblem(message: String) -> Element {
    rsx! {
        div { class: "tw:mb-1.5 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-4 tw:text-xs tw:text-status-error-foreground tw:last:mb-0",
            "{message}"
        }
    }
}

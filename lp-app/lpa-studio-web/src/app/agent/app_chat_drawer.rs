//! [`AppChatDrawer`]: the app chat as a right-side drawer over the page.
//!
//! Mounted once by the web app, outside every route's body, so it stays
//! open across navigation (plan A2): opening a project, going to Devices
//! and coming back never closes it or drops a half-typed message. Its open
//! flag and draft are web-local chrome ([`super::AppChatChrome`]); the
//! conversation is core's.
//!
//! It overlays the right edge, under the site chrome, rather than
//! reflowing the page: the
//! workbench is a full-height frame with docks of its own, and a drawer
//! that pushed it would rearrange the room every time it opened.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAppAgentView};

use super::app_chat_pane::{APP_CHAT_TITLE, AppChatPane};
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AppChatDrawer(
    view: UiAppAgentView,
    open: Signal<bool>,
    draft: Signal<String>,
    /// Open tool rows expanded on first render (stories).
    #[props(default = false)]
    tool_rows_expanded: bool,
    /// Stories: pin the drawer to its story frame's right edge (a
    /// positioned box) instead of the viewport's.
    #[props(default = false)]
    inline: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    #[props(default)] on_connect: Option<EventHandler<()>>,
    #[props(default)] connect_error: Option<String>,
) -> Element {
    let mut open = open;
    if !open() {
        return rsx! {};
    }
    let frame = if inline {
        INLINE_FRAME_CLASS
    } else {
        FIXED_FRAME_CLASS
    };
    rsx! {
        aside {
            class: "{frame} tw:flex tw:min-h-0 tw:flex-col tw:overflow-hidden tw:border-l tw:border-border tw:bg-card tw:shadow-2xl",
            aria_label: "{APP_CHAT_TITLE}",
            header { class: "tw:flex tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2",
                span { class: "tw:inline-flex tw:text-strong-foreground",
                    StudioIcon { name: StudioIconName::Agent, size: 16 }
                }
                p { class: "tw:m-0 tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                    "{APP_CHAT_TITLE}"
                }
                button {
                    class: "tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-transparent tw:bg-transparent tw:text-muted-foreground tw:transition tw:duration-300 tw:hover:border-border-subtle tw:hover:text-strong-foreground",
                    r#type: "button",
                    title: "Close the assistant (the conversation stays)",
                    aria_label: "Close the assistant",
                    onclick: move |_| open.set(false),
                    "✕"
                }
            }
            AppChatPane {
                view,
                draft,
                tool_rows_expanded,
                on_action,
                on_connect,
                connect_error,
            }
        }
    }
}

/// The product's drawer: fixed to the viewport's right edge, from just
/// under the site chrome to the bottom, over everything but the browser's
/// top layer. It starts under the chrome (which sits at the same height on
/// every route) so the bar's right cluster — the chat button that closes
/// it, and the AI settings the not-configured state points at — stays
/// reachable while it is open.
const FIXED_FRAME_CLASS: &str =
    "tw:fixed tw:top-[53px] tw:bottom-0 tw:right-0 tw:z-40 tw:w-[min(440px,100vw)] tw:border-t";

/// The stories' drawer: the same box, against its frame's right edge.
const INLINE_FRAME_CLASS: &str = "tw:absolute tw:inset-y-0 tw:right-0 tw:w-[min(440px,100%)]";

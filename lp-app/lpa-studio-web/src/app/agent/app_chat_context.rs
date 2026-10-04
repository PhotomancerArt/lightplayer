//! [`AppChatChrome`]: the app chat's web-local chrome state — whether the
//! drawer is open, and the composer draft — shared by the header button,
//! the home page's front door and the drawer itself.
//!
//! Like a popover's open flag, none of it is core state (plan A2): the
//! conversation, its cards and its cost live in core
//! (`UiStudioView::app_agent`); this is only where the window stands and
//! the half-typed text, owned high enough (the web app) that it survives
//! navigation and the drawer closing.

use dioxus::prelude::*;

/// The app chat's chrome state, provided once by the web app.
#[derive(Clone, Copy)]
pub(crate) struct AppChatChrome {
    /// The drawer is open.
    pub open: Signal<bool>,
    /// The composer's draft (the front door and the drawer share it, so a
    /// request typed on the home page is still there after connecting a
    /// provider).
    pub draft: Signal<String>,
}

/// Provide the chrome state to everything below the caller.
pub(crate) fn use_provide_app_chat_chrome() -> AppChatChrome {
    use_context_provider(|| AppChatChrome {
        open: Signal::new(false),
        draft: Signal::new(String::new()),
    })
}

/// The chrome state, when a web app provides it (stories do not).
pub(crate) fn use_app_chat_chrome() -> Option<AppChatChrome> {
    use_hook(try_consume_context::<AppChatChrome>)
}

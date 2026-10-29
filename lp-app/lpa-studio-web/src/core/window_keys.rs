//! Window-level keydown plumbing: the app-wide shortcut listener (the
//! command palette's ⌘K).
//!
//! Mechanics: a listener installs once per mount ([`use_hook`]) and is
//! removed on unmount (Drop — the popover auto-update idiom). The raw JS
//! closure routes through a Dioxus [`Callback`] created by
//! [`use_callback`], which replaces its inner closure every render — so
//! the handler always sees the current render's captures without any
//! signal mirroring, and signal writes made inside it carry runtime
//! context (the resize-observer precedent).

use dioxus::prelude::*;

/// Install a window keydown listener for a chord that works EVERYWHERE,
/// editable fields included, for the life of the calling component. It
/// listens in the capture phase so a field that stops the event's
/// propagation (CodeMirror) cannot swallow it first. The handler sees
/// every keydown, so it must match its chord narrowly and leave the rest
/// alone.
pub(crate) fn use_window_shortcut(handler: impl FnMut(web_sys::KeyboardEvent) + 'static) {
    let callback = use_callback(handler);
    #[cfg(target_arch = "wasm32")]
    use_hook(move || std::rc::Rc::new(WindowShortcut::install(callback)));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = callback;
}

/// The capture-phase keydown listener behind [`use_window_shortcut`],
/// removed from the window when the last handle drops.
#[cfg(target_arch = "wasm32")]
struct WindowShortcut {
    window: web_sys::Window,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::KeyboardEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl WindowShortcut {
    fn install(handler: Callback<web_sys::KeyboardEvent>) -> Option<Self> {
        use wasm_bindgen::JsCast as _;

        let window = web_sys::window()?;
        let callback =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                handler.call(event);
            }) as Box<dyn FnMut(_)>);
        window
            .add_event_listener_with_callback_and_bool(
                "keydown",
                callback.as_ref().unchecked_ref(),
                true,
            )
            .ok()?;
        Some(Self { window, callback })
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WindowShortcut {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast as _;
        let _ = self.window.remove_event_listener_with_callback_and_bool(
            "keydown",
            self.callback.as_ref().unchecked_ref(),
            true,
        );
    }
}

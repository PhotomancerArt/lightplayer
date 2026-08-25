//! View-scoped WINDOW hotkeys for the workbench centers.
//!
//! Both editing views used to carry their grammar as an `onkeydown` on the
//! center div (`tabindex: 0`) — and nothing ever refocused that div, so
//! the moment focus landed in a dock (clicking a port free-run is a NORMAL
//! step of the walk-up loop) every hotkey silently died until the canvas
//! was clicked again. The grammar now rides a window-level keydown
//! listener that is active exactly while the view's center is mounted.
//!
//! Mechanics: the listener installs once per mount ([`use_hook`]) and is
//! removed on unmount (Drop — the popover auto-update idiom). The raw JS
//! closure routes through a Dioxus [`Callback`] created by
//! [`use_callback`], which replaces its inner closure every render — so
//! the handler always sees the current render's captures (surface,
//! selection) without any signal mirroring, and signal writes made inside
//! it carry runtime context (the resize-observer precedent).
//!
//! Guard: events targeting editable controls (`input`, `textarea`,
//! `select`, `contenteditable`) never reach the handler — typing in a
//! field is not a verb.

use dioxus::prelude::*;

/// Parse a raw window keydown into the canvas crate's [`EditorKeyInput`]
/// (key string + modifier flags — target-independent, so host-built view
/// tests compile the same call sites).
pub(crate) fn editor_key_input(
    event: &web_sys::KeyboardEvent,
) -> lpa_mapping_editor::EditorKeyInput {
    lpa_mapping_editor::EditorKeyInput::from_raw(
        &event.key(),
        event.meta_key(),
        event.ctrl_key(),
        event.shift_key(),
        event.alt_key(),
    )
}

/// Install a window keydown listener for the life of the calling
/// component. `handler` is re-captured every render (see module doc);
/// editable-target events are filtered before it runs.
pub(crate) fn use_window_keydown(handler: impl FnMut(web_sys::KeyboardEvent) + 'static) {
    let callback = use_callback(handler);
    #[cfg(target_arch = "wasm32")]
    use_hook(move || std::rc::Rc::new(WindowKeydown::install(callback)));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = callback;
}

#[cfg(target_arch = "wasm32")]
struct WindowKeydown {
    window: web_sys::Window,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::KeyboardEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl WindowKeydown {
    fn install(handler: Callback<web_sys::KeyboardEvent>) -> Option<Self> {
        use wasm_bindgen::JsCast as _;

        let window = web_sys::window()?;
        let callback =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                if lpa_mapping_editor::event_targets_editable(&event) {
                    return;
                }
                handler.call(event);
            }) as Box<dyn FnMut(_)>);
        window
            .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())
            .ok()?;
        Some(Self { window, callback })
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WindowKeydown {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast as _;
        let _ = self
            .window
            .remove_event_listener_with_callback("keydown", self.callback.as_ref().unchecked_ref());
    }
}

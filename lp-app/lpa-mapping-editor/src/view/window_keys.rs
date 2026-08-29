//! Window-level key tracking for the canvas: the space-bar pan state and
//! the editable-target guard hosts share.
//!
//! The canvas cannot rely on component-level `onkeydown` for a HELD key —
//! focus wanders into docks and inputs, and a keyup that lands elsewhere
//! would wedge the state. So space tracking installs window listeners for
//! the life of the mount (the popover auto-update idiom: closures held in
//! a struct, removed on Drop), routing writes through a Dioxus [`Callback`]
//! so signal subscribers are actually notified (raw JS callbacks have no
//! runtime context — the resize-observer precedent).

use dioxus::prelude::*;

/// True when the event's target is an editable control — `input`,
/// `textarea`, `select`, or a `contenteditable` region. Window-level key
/// grammars must ignore those: typing in a field is never a canvas verb.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn event_targets_editable(event: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast as _;
    let Some(target) = event.target() else {
        return false;
    };
    let Some(element) = target.dyn_ref::<web_sys::Element>() else {
        return false;
    };
    if matches!(element.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
        return true;
    }
    element
        .dyn_ref::<web_sys::HtmlElement>()
        .is_some_and(web_sys::HtmlElement::is_content_editable)
}

/// Space-held state for the canvas pan grammar: while true, a left-drag
/// pans (the Figma/Illustrator hand) instead of running the tool or
/// fixture grammar. Cleared on keyup and on window blur (a missed keyup —
/// ⌘-tab away mid-hold — must not wedge the hand on).
pub(crate) fn use_space_held() -> Signal<bool> {
    let held = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    {
        let set_held = use_callback(move |value: bool| {
            let mut held = held;
            if *held.peek() != value {
                held.set(value);
            }
        });
        use_hook(move || std::rc::Rc::new(SpaceTracker::install(set_held)));
    }
    held
}

#[cfg(target_arch = "wasm32")]
struct SpaceTracker {
    window: web_sys::Window,
    keydown: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::KeyboardEvent)>,
    keyup: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::KeyboardEvent)>,
    blur: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl SpaceTracker {
    fn install(set_held: Callback<bool>) -> Option<Self> {
        use wasm_bindgen::JsCast as _;
        use wasm_bindgen::closure::Closure;

        let window = web_sys::window()?;
        let keydown = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if event.key() != " " || event_targets_editable(&event) {
                return;
            }
            // Space's browser default scrolls the page / activates the
            // focused button; while it means "hand", neither should fire.
            event.prevent_default();
            set_held.call(true);
        }) as Box<dyn FnMut(_)>);
        let keyup = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if event.key() == " " {
                set_held.call(false);
            }
        }) as Box<dyn FnMut(_)>);
        let blur = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_held.call(false);
        }) as Box<dyn FnMut(_)>);
        window
            .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
            .ok()?;
        if window
            .add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())
            .is_err()
        {
            let _ = window
                .remove_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref());
            return None;
        }
        if window
            .add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref())
            .is_err()
        {
            let _ = window
                .remove_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref());
            let _ =
                window.remove_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref());
            return None;
        }
        Some(Self {
            window,
            keydown,
            keyup,
            blur,
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for SpaceTracker {
    fn drop(&mut self) {
        let _ = self.window.remove_event_listener_with_callback("keydown", {
            use wasm_bindgen::JsCast as _;
            self.keydown.as_ref().unchecked_ref()
        });
        let _ = self.window.remove_event_listener_with_callback("keyup", {
            use wasm_bindgen::JsCast as _;
            self.keyup.as_ref().unchecked_ref()
        });
        let _ = self.window.remove_event_listener_with_callback("blur", {
            use wasm_bindgen::JsCast as _;
            self.blur.as_ref().unchecked_ref()
        });
    }
}

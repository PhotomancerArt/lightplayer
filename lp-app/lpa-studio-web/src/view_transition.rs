//! SPIKE A (scene direction discovery): same-document View Transitions
//! around ARRANGEMENT flips (gallery ⇄ editor).
//!
//! The browser pairs elements by `view-transition-name` across the DOM
//! update and morphs their boxes on the compositor — shared-element
//! continuity WITHOUT owning layout (the "ride the browser engineers'
//! coattails" bet; the alternative is a custom top-level layout engine).
//!
//! The one integration trick: `document.startViewTransition(update)`
//! snapshots OLD synchronously, then waits for `update`'s promise before
//! snapshotting NEW. Dioxus renders on its own schedule after a signal
//! write, so the update callback resolves after a double
//! `requestAnimationFrame` — by which point the diff has committed. If
//! that assumption is wrong the morph degrades to a crossfade of
//! identical frames (visible, not broken) — one of the spike's
//! go/no-go questions.
//!
//! Unsupported browsers (and non-wasm builds) run the update directly:
//! the transition is pure progressive enhancement.

/// Run `update` inside a view transition when the platform has one,
/// directly otherwise. `update` must synchronously enqueue the state
/// change (a signal write); the glue gives the framework two frames to
/// commit before the NEW snapshot is taken.
#[cfg(target_arch = "wasm32")]
pub(crate) fn with_view_transition(update: impl FnOnce() + 'static) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;

    let closure = Closure::once(update);
    js::start_view_transition(closure.as_ref().unchecked_ref());
    // the browser holds the callback through the transition; leaking one
    // closure per arrangement flip is bounded by user gestures
    closure.forget();
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn with_view_transition(update: impl FnOnce() + 'static) {
    update();
}

#[cfg(target_arch = "wasm32")]
mod js {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = r#"
export function start_view_transition(update) {
  if (!document.startViewTransition) {
    update();
    return;
  }
  document.startViewTransition(
    () =>
      new Promise((resolve) => {
        update();
        // two frames: one for the framework to flush, one for layout
        requestAnimationFrame(() => requestAnimationFrame(resolve));
      }),
  );
}
"#)]
    extern "C" {
        pub fn start_view_transition(update: &js_sys::Function);
    }
}

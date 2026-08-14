//! The mounted-svg anchor: pointer/wheel coordinates and viewport
//! measurement both key off the canvas element's live bounding rect.

use dioxus::prelude::*;

/// Host-side fallback offset for pointer math when no mounted element is
/// available (tests, SSR): the standalone page's header height. In the
/// browser the mounted svg's live bounding rect anchors coordinates instead
/// — the editor can sit anywhere in a scrolling page, so a fixed offset
/// cannot work there.
const HEADER_OFFSET: f32 = 49.0;

/// Handle to the mounted canvas svg. Pointer and wheel coordinates anchor to
/// its live bounding rect (queried per event — scroll-proof), and the host
/// viewport size is measured from the same rect.
#[derive(Clone, Default, PartialEq)]
pub struct CanvasAnchor {
    #[cfg(target_arch = "wasm32")]
    element: Option<web_sys::Element>,
}

impl CanvasAnchor {
    /// Anchor to a mounted element (the host's `onmounted` handler).
    #[cfg(target_arch = "wasm32")]
    pub fn from_element(element: web_sys::Element) -> Self {
        Self {
            element: Some(element),
        }
    }

    /// Top-left of the canvas in client coordinates.
    pub fn origin(&self) -> [f32; 2] {
        #[cfg(target_arch = "wasm32")]
        if let Some(element) = &self.element {
            let rect = element.get_bounding_client_rect();
            return [rect.left() as f32, rect.top() as f32];
        }
        [0.0, HEADER_OFFSET]
    }

    /// Measured canvas size in CSS pixels, when mounted.
    pub fn size(&self) -> Option<[f32; 2]> {
        #[cfg(target_arch = "wasm32")]
        if let Some(element) = &self.element {
            let rect = element.get_bounding_client_rect();
            let size = [rect.width() as f32, rect.height() as f32];
            if size[0] > 0.0 && size[1] > 0.0 {
                return Some(size);
            }
        }
        None
    }
}

/// Owns the canvas's ResizeObserver and its JS callback; disconnects on
/// drop. The svg's `onresize` attribute never fires in this stack, so this
/// is what keeps the measured viewport current across embed-box changes.
#[cfg(target_arch = "wasm32")]
pub(crate) struct CanvasResizeObserver {
    observer: web_sys::ResizeObserver,
    _callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::js_sys::Array)>,
}

#[cfg(target_arch = "wasm32")]
impl CanvasResizeObserver {
    pub(crate) fn install(
        element: &web_sys::Element,
        mut measure: impl FnMut() + 'static,
    ) -> Option<Self> {
        use wasm_bindgen::JsCast;
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::js_sys::Array)>::new(
            move |_entries: web_sys::js_sys::Array| measure(),
        );
        let observer = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()).ok()?;
        observer.observe(element);
        Some(Self {
            observer,
            _callback: callback,
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for CanvasResizeObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

/// Route the rest of this pointer stream to the pressed element even when
/// the cursor crosses overlays (rail, popover) or leaves the window — drags
/// must not die at the first overlay edge.
pub fn capture_pointer(evt: &Event<PointerData>) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(web_event) = evt.data().downcast::<web_sys::PointerEvent>()
            && let Some(target) = web_event.target()
            && let Ok(element) = target.dyn_into::<web_sys::Element>()
        {
            let _ = element.set_pointer_capture(web_event.pointer_id());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = evt;
    }
}

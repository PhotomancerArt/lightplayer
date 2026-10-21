//! Drawing on a live picture: the pointer surface over a [`LampView`] and
//! the rings under each finger (plan `drawing-on-the-picture`, P6).
//!
//! The web reports where the fingers are and draws the rings; everything
//! else is core's. Each pointer change sends one `StudioCommand::Stroke`
//! with every live pointer, as `0..1` of the drawn picture box (the box
//! [`LampPictureFrame::fit`] paints into — the same fit the canvas uses),
//! and an empty list when the last finger lifts. Core turns that into paced
//! momentary touch writes (D11): the web builds no action.
//!
//! The rings are drawn here at once, on an overlay, never by repainting the
//! lamp canvas: they say "your finger is here" while the lamps behind catch
//! up. A ring fades out within [`RING_FADE_MS`] of its finger lifting.
//!
//! A tap is a one-sample stroke: down reports the finger, up reports the
//! release, and core sends the dab (Q6). `touch-action: none` is set only on
//! a drawable picture, so a phone still scrolls past every other one.
//!
//! [`LampView`]: super::lamp_view::LampView

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::{
    ControlDisplayLayout, LampPictureFrame, UiDrawable, UiStroke, UiStrokePointer,
};

/// How long a lifted finger's ring takes to fade.
pub(crate) const RING_FADE_MS: u32 = 250;

/// Where a drawable picture reports its strokes: the app provides it (the
/// command channel into core), stories do not, and a picture with no
/// reporter is not drawn on.
#[derive(Clone, Copy)]
pub(crate) struct StrokeReporter(pub Callback<UiStroke>);

/// The app's stroke reporter, when there is one.
pub(crate) fn use_stroke_reporter() -> Option<StrokeReporter> {
    use_hook(try_consume_context::<StrokeReporter>)
}

/// One ring under a finger, in CSS pixels of the drawing surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DrawRing {
    pub pointer: i32,
    pub x: f64,
    pub y: f64,
    /// The finger lifted: the ring is fading out.
    pub lifted: bool,
}

/// The pointer surface over a lamp canvas: takes the picture's pointers,
/// reports them, and draws the rings. `children` is the canvas.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn LampDrawSurface(
    drawable: UiDrawable,
    layout: Rc<ControlDisplayLayout>,
    /// Rings to show without a pointer (stories).
    #[props(default)]
    rings: Vec<DrawRing>,
    children: Element,
) -> Element {
    let reporter = use_stroke_reporter();
    let ControlDisplayLayout::Layout2d(layout) = layout.as_ref();
    let frame = LampPictureFrame::of(layout);
    let fingers = use_hook(|| Rc::new(RefCell::new(Vec::<(i32, [f64; 2], [f64; 2])>::new())));
    let mut live_rings = use_signal(Vec::<DrawRing>::new);

    let surface = drawable.surface;
    // Report every live finger as `0..1` of the picture box; an empty list
    // when the last one lifted. `size` is the surface's CSS box.
    let report = {
        let fingers = fingers.clone();
        move || {
            let Some(reporter) = reporter else {
                return;
            };
            let pointers = fingers
                .borrow()
                .iter()
                .filter_map(|(pointer, point, size)| {
                    let fit = frame.as_ref()?.fit([0.0, 0.0, size[0], size[1]]);
                    let [x, y] = fit.picture_point(*point);
                    Some(UiStrokePointer {
                        pointer: *pointer as u32,
                        x,
                        y,
                    })
                })
                .collect();
            reporter.0.call(UiStroke::new(surface, pointers));
        }
    };

    let down = {
        let fingers = fingers.clone();
        let report = report.clone();
        move |event: Event<PointerData>| {
            event.prevent_default();
            capture_pointer(&event);
            let (pointer, point, size) = pointer_sample(&event);
            {
                let mut fingers = fingers.borrow_mut();
                fingers.retain(|(id, _, _)| *id != pointer);
                fingers.push((pointer, point, size));
            }
            live_rings.with_mut(|rings| {
                rings.retain(|ring| ring.pointer != pointer);
                rings.push(DrawRing {
                    pointer,
                    x: point[0],
                    y: point[1],
                    lifted: false,
                });
            });
            report();
        }
    };
    let moved = {
        let fingers = fingers.clone();
        let report = report.clone();
        move |event: Event<PointerData>| {
            let (pointer, point, size) = pointer_sample(&event);
            let known = {
                let mut fingers = fingers.borrow_mut();
                match fingers.iter_mut().find(|(id, _, _)| *id == pointer) {
                    Some(finger) => {
                        finger.1 = point;
                        finger.2 = size;
                        true
                    }
                    None => false,
                }
            };
            if !known {
                return;
            }
            live_rings.with_mut(|rings| {
                if let Some(ring) = rings.iter_mut().find(|ring| ring.pointer == pointer) {
                    ring.x = point[0];
                    ring.y = point[1];
                }
            });
            report();
        }
    };
    let lifted = {
        let fingers = fingers.clone();
        let report = report.clone();
        move |event: Event<PointerData>| {
            let pointer = event.data().pointer_id();
            let was_down = {
                let mut fingers = fingers.borrow_mut();
                let before = fingers.len();
                fingers.retain(|(id, _, _)| *id != pointer);
                fingers.len() != before
            };
            if !was_down {
                return;
            }
            live_rings.with_mut(|rings| {
                if let Some(ring) = rings.iter_mut().find(|ring| ring.pointer == pointer) {
                    ring.lifted = true;
                }
            });
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(RING_FADE_MS).await;
                live_rings.with_mut(|rings| {
                    rings.retain(|ring| !(ring.pointer == pointer && ring.lifted));
                });
            });
            report();
        }
    };
    let cancelled = lifted.clone();

    let shown: Vec<DrawRing> = rings
        .iter()
        .chain(live_rings.read().iter())
        .copied()
        .collect();
    rsx! {
        div {
            class: "ux-lamp-draw",
            "data-drawable": "1",
            onpointerdown: down,
            onpointermove: moved,
            onpointerup: lifted,
            onpointercancel: cancelled,
            {children}
            svg { class: "ux-lamp-draw-rings", "aria-hidden": "true",
                for ring in shown {
                    circle {
                        key: "{ring.pointer}",
                        class: if ring.lifted { "ux-lamp-draw-ring ux-lamp-draw-ring-lifted" } else { "ux-lamp-draw-ring" },
                        cx: "{ring.x}",
                        cy: "{ring.y}",
                        r: "14",
                    }
                }
            }
        }
    }
}

/// `(pointer id, point, surface size)` of a pointer event, in the drawing
/// surface's CSS pixels. Captured pointers keep reporting past its edge.
fn pointer_sample(event: &Event<PointerData>) -> (i32, [f64; 2], [f64; 2]) {
    let data = event.data();
    let point = data.element_coordinates();
    (
        data.pointer_id(),
        [point.x, point.y],
        surface_size(event).unwrap_or([1.0, 1.0]),
    )
}

/// The drawing surface's CSS box. Dioxus delegates events, so the event's
/// `currentTarget` is the app's root, not the surface: read the surface off
/// the real target instead ([`surface_element`]).
fn surface_size(event: &Event<PointerData>) -> Option<[f64; 2]> {
    let surface = surface_element(event)?;
    Some([
        f64::from(surface.client_width()).max(1.0),
        f64::from(surface.client_height()).max(1.0),
    ])
}

/// Keep a finger's events on the surface while it drags past the edge. The
/// capture goes on the surface itself: capturing on `currentTarget` (the
/// root, under delegation) sent every later move and lift to the root, and
/// the surface heard only the press.
fn capture_pointer(event: &Event<PointerData>) {
    use dioxus::web::WebEventExt;

    let Some(web_event) = event.data().try_as_web_event() else {
        return;
    };
    if let Some(surface) = surface_element(event) {
        let _ = surface.set_pointer_capture(web_event.pointer_id());
    }
}

/// The `.ux-lamp-draw` element the event happened in: its target or the
/// target's nearest such ancestor.
fn surface_element(event: &Event<PointerData>) -> Option<web_sys::Element> {
    use dioxus::web::WebEventExt;
    use wasm_bindgen::JsCast;

    let web_event = event.data().try_as_web_event()?;
    let target = web_event.target()?.dyn_into::<web_sys::Element>().ok()?;
    target.closest(".ux-lamp-draw").ok().flatten()
}

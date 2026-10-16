//! The background dots, on a screen-space canvas under everything: quiet,
//! spaced in world units, stepping to a coarser spacing (×5) when they
//! would crowd closer than a few pixels; each dot one size on screen.
//! Redrawn when the camera or the view's size changes.

use dioxus::prelude::*;
use lab_mapping_model::{Camera, Vec2};
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

use crate::dense_canvas::{canvas_id, context, device_pixel_ratio};

/// The finest spacing, in world units…
const SPACING: f64 = 10.0;
/// …made coarser (×5 at a time) until the dots are at least this far apart
/// on screen.
const MIN_GAP_PX: f64 = 16.0;
/// A dot's radius on screen.
const DOT_PX: f64 = 1.0;

#[component]
pub fn DotsLayer(camera: Camera, viewport: Vec2) -> Element {
    let id = use_hook(|| canvas_id("dots"));
    let canvas = id.clone();
    spawn(async move {
        for _ in 0..24 {
            if draw(&canvas, camera, viewport).is_some() {
                return;
            }
            gloo_timers::future::TimeoutFuture::new(16).await;
        }
    });
    rsx! {
        canvas { id: "{id}", class: "dots" }
    }
}

fn draw(id: &str, camera: Camera, viewport: Vec2) -> Option<()> {
    let canvas = web_sys::window()?
        .document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlCanvasElement>()
        .ok()?;
    let dpr = device_pixel_ratio();
    let (w, h) = (
        (viewport.x * dpr).round().max(1.0) as u32,
        (viewport.y * dpr).round().max(1.0) as u32,
    );
    if canvas.width() != w || canvas.height() != h {
        canvas.set_width(w);
        canvas.set_height(h);
    }
    let ctx = context(&canvas)?;
    ctx.clear_rect(0.0, 0.0, w as f64, h as f64);
    let mut step = SPACING;
    while step * camera.scale < MIN_GAP_PX {
        step *= 5.0;
    }
    let from = camera.to_world(Vec2::new(0.0, 0.0));
    let to = camera.to_world(viewport);
    let (x0, y0) = ((from.x / step).floor() as i64, (from.y / step).floor() as i64);
    let (x1, y1) = ((to.x / step).ceil() as i64, (to.y / step).ceil() as i64);
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.09)");
    let r = DOT_PX * dpr;
    for j in y0..=y1 {
        for i in x0..=x1 {
            let p = camera.to_screen(Vec2::new(i as f64 * step, j as f64 * step));
            ctx.fill_rect(p.x * dpr - r, p.y * dpr - r, 2.0 * r, 2.0 * r);
        }
    }
    Some(())
}

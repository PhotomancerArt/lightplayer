//! A render source's picture from the board frame, on a `<canvas>` at its
//! own pixels and stretched by CSS: a shader's card shows it, and a
//! fixture's face shows it faintly behind its lamps (plan D11). A source
//! with no picture in the frame draws nothing.

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, NodeId, Picture};
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{HtmlCanvasElement, ImageData};

use crate::dense_canvas::{canvas_id, context};

#[component]
pub fn PictureCanvas(
    source: NodeId,
    frame: ReadSignal<BoardFrame>,
    class: &'static str,
    /// Where it sits in its face, as CSS (`left`, `top`, `width`, `height`).
    style: String,
) -> Element {
    let id = use_hook(|| canvas_id("picture"));
    // Read here, so a new frame draws again.
    let picture = frame.read().picture(&source).cloned();
    let Some(picture) = picture else {
        return rsx! {};
    };
    let canvas = id.clone();
    spawn(async move {
        for _ in 0..24 {
            if put(&canvas, &picture).is_ok() {
                return;
            }
            gloo_timers::future::TimeoutFuture::new(16).await;
        }
    });
    rsx! {
        canvas { id: "{id}", class: "{class}", style: "{style}" }
    }
}

/// Draw `picture` on the canvas with this id, its backing store the
/// picture's own size.
fn put(canvas_id: &str, picture: &Picture) -> Result<(), String> {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(canvas_id))
        .and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
        .ok_or("no canvas yet")?;
    if canvas.width() != picture.width || canvas.height() != picture.height {
        canvas.set_width(picture.width);
        canvas.set_height(picture.height);
    }
    let mut rgba = Vec::with_capacity((picture.width * picture.height * 4) as usize);
    for px in picture.rgb.chunks_exact(3) {
        rgba.extend([px[0], px[1], px[2], 255]);
    }
    let ctx = context(&canvas).ok_or("no 2d context")?;
    let image =
        ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), picture.width, picture.height)
            .map_err(|_| "no image data")?;
    ctx.put_image_data(&image, 0.0, 0.0)
        .map_err(|_| "put failed".to_string())
}

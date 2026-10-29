//! How wide a label is, measured with the browser's own fonts, so
//! `lab-arrows` sizes each label's box to the text actually drawn.

use std::cell::RefCell;

use wasm_bindgen::JsCast;

thread_local! {
    static CONTEXT: RefCell<Option<web_sys::CanvasRenderingContext2d>> = const { RefCell::new(None) };
}

/// The labels' font: the page's own sans (`--font` in `lab.css`).
const FONT_FAMILY: &str =
    "Inter, ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", sans-serif";

/// The width of `text` at `size` pixels, as the browser would draw it; an
/// estimate when there is no browser to ask.
pub fn measure(text: &str, size: f64) -> f64 {
    CONTEXT.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = context();
        }
        match c.as_ref() {
            Some(ctx) => {
                ctx.set_font(&format!("{size}px {FONT_FAMILY}"));
                ctx.measure_text(text)
                    .map(|m| m.width())
                    .unwrap_or_else(|_| estimate(text, size))
            }
            None => estimate(text, size),
        }
    })
}

fn estimate(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * 0.56 * size
}

fn context() -> Option<web_sys::CanvasRenderingContext2d> {
    let canvas = web_sys::window()?
        .document()?
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?;
    canvas.get_context("2d").ok()??.dyn_into().ok()
}

//! Dense layers: lamps and pixels, drawn on a `<canvas>` inside their face
//! (Q4), never as DOM. The radiance dome has 22,000 of each; as SVG they
//! were ~50,000 elements and the page panned at half a second a frame.
//!
//! A face's canvas is sized in world units like the face, so the world
//! layer's transform carries it when the canvas pans: panning never
//! redraws it. Its backing store is its size on screen × the device pixel
//! ratio, so it is sharp at the zoom it was drawn at, and it is drawn again
//! when the zoom moves a step (`zoom_step`) or the colours tick. Drawing is
//! one pass in Rust into a pixel buffer and one `putImageData`: tens of
//! thousands of canvas calls across the JS boundary would cost more than
//! the drawing.

use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

/// The most a face's canvas holds, each way and in all, in device pixels:
/// past this a zoomed-in face is drawn softer rather than allocating
/// without end.
const MAX_SIDE: f64 = 4096.0;
const MAX_PIXELS: f64 = 4_000_000.0;

/// The zoom a face is drawn at: the camera's, in steps of an eighth of a
/// doubling, so a pinch redraws faces a few times rather than every event.
pub fn zoom_step(scale: f64) -> f64 {
    2f64.powf((scale.max(1e-6).log2() * 8.0).round() / 8.0)
}

/// The screen's device pixel ratio.
pub fn device_pixel_ratio() -> f64 {
    web_sys::window().map_or(1.0, |w| w.device_pixel_ratio())
}

/// A pixel buffer for one face, `k` device pixels per world unit.
pub struct Raster {
    pub w: u32,
    pub h: u32,
    /// Device pixels per world unit.
    pub k: f64,
    buf: Vec<u8>,
}

impl Raster {
    /// A clear buffer for a face `size` world units big, drawn at `zoom`.
    pub fn new(size_x: f64, size_y: f64, zoom: f64) -> Raster {
        let mut k = zoom * device_pixel_ratio();
        let (sx, sy) = (size_x.max(1e-6), size_y.max(1e-6));
        k = k.min(MAX_SIDE / sx).min(MAX_SIDE / sy);
        k = k.min((MAX_PIXELS / (sx * sy)).sqrt());
        let w = (sx * k).ceil().max(1.0) as u32;
        let h = (sy * k).ceil().max(1.0) as u32;
        Raster {
            w,
            h,
            k,
            buf: vec![0; (w * h * 4) as usize],
        }
    }

    /// A filled disc at world `(x, y)`, from a [`Stamp`]: its centre on
    /// the nearest device pixel. The lamp's colour and its rim's are worked
    /// out once; each pixel is then a write, or a blend where it lands on
    /// something already drawn.
    pub fn stamp(&mut self, x: f64, y: f64, stamp: &Stamp, rgba: [f64; 4]) {
        let (cx, cy) = ((x * self.k).floor() as i64, (y * self.k).floor() as i64);
        let n = stamp.side as i64;
        let half = n / 2;
        let body = to_u8(rgba);
        let rim = stamp.rim.map(|r| to_u8(mix(rgba, r, r[3])));
        let alpha = rgba[3].clamp(0.0, 1.0);
        for j in 0..n {
            let py = cy + j - half;
            if py < 0 || py >= self.h as i64 {
                continue;
            }
            let row = (py as u32 * self.w) as usize;
            for i in 0..n {
                let px = cx + i - half;
                if px < 0 || px >= self.w as i64 {
                    continue;
                }
                let (cover, edge) = stamp.cells[(j * n + i) as usize];
                if cover <= 0.0 {
                    continue;
                }
                let c = match rim {
                    Some(r) if edge > 0.5 => r,
                    _ => body,
                };
                let a = (cover * alpha * 255.0) as u8;
                let at = (row + px as usize) * 4;
                if self.buf[at + 3] == 0 {
                    self.buf[at] = c[0];
                    self.buf[at + 1] = c[1];
                    self.buf[at + 2] = c[2];
                    self.buf[at + 3] = a;
                } else {
                    self.blend(
                        px,
                        py,
                        [c[0] as f64, c[1] as f64, c[2] as f64, 1.0],
                        a as f64 / 255.0,
                    );
                }
            }
        }
    }

    /// A ring at world `(x, y)`: radius `r` and line width `width`, both
    /// device pixels.
    pub fn ring(&mut self, x: f64, y: f64, r: f64, width: f64, rgba: [f64; 4]) {
        let (cx, cy) = (x * self.k, y * self.k);
        let reach = r + width + 1.0;
        let (x0, x1) = (
            (cx - reach).floor().max(0.0) as i64,
            (cx + reach).ceil().min(self.w as f64) as i64,
        );
        let (y0, y1) = (
            (cy - reach).floor().max(0.0) as i64,
            (cy + reach).ceil().min(self.h as f64) as i64,
        );
        for py in y0..y1 {
            for px in x0..x1 {
                let d = ((px as f64 + 0.5 - cx).powi(2) + (py as f64 + 0.5 - cy).powi(2)).sqrt();
                let cover = (width / 2.0 + 0.5 - (d - r).abs()).clamp(0.0, 1.0);
                if cover > 0.0 {
                    self.blend(px, py, rgba, cover * rgba[3]);
                }
            }
        }
    }

    /// A filled box, world units, less `gap` device pixels at its right and
    /// bottom.
    pub fn rect(&mut self, min: (f64, f64), max: (f64, f64), gap: f64, rgba: [f64; 4]) {
        let x0 = (min.0 * self.k).round().max(0.0) as i64;
        let y0 = (min.1 * self.k).round().max(0.0) as i64;
        let x1 = ((max.0 * self.k).round() - gap).min(self.w as f64) as i64;
        let y1 = ((max.1 * self.k).round() - gap).min(self.h as f64) as i64;
        // Never nothing: a square too small to see is still a dot.
        let (x1, y1) = (x1.max(x0 + 1), y1.max(y0 + 1));
        let c = to_u8(rgba);
        let a = (rgba[3].clamp(0.0, 1.0) * 255.0) as u8;
        for py in y0.max(0)..y1.min(self.h as i64) {
            let row = (py as u32 * self.w) as usize;
            for px in x0.max(0)..x1.min(self.w as i64) {
                let at = (row + px as usize) * 4;
                self.buf[at..at + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
            }
        }
    }

    fn blend(&mut self, px: i64, py: i64, c: [f64; 4], alpha: f64) {
        let i = ((py as u32 * self.w + px as u32) * 4) as usize;
        let a = alpha.clamp(0.0, 1.0);
        let under = self.buf[i + 3] as f64 / 255.0;
        let out = a + under * (1.0 - a);
        if out <= 0.0 {
            return;
        }
        for ch in 0..3 {
            let below = self.buf[i + ch] as f64;
            let v = (c[ch] * a + below * under * (1.0 - a)) / out;
            self.buf[i + ch] = v.round().clamp(0.0, 255.0) as u8;
        }
        self.buf[i + 3] = (out * 255.0).round() as u8;
    }

    /// Put the buffer on the canvas with this element id, sizing its
    /// backing store to match.
    pub fn put(&self, canvas_id: &str) -> Result<(), String> {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(canvas_id))
            .and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
            .ok_or("no canvas yet")?;
        if canvas.width() != self.w || canvas.height() != self.h {
            canvas.set_width(self.w);
            canvas.set_height(self.h);
        }
        let ctx = context(&canvas).ok_or("no 2d context")?;
        let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&self.buf), self.w, self.h)
            .map_err(|_| "no image data")?;
        ctx.put_image_data(&image, 0.0, 0.0)
            .map_err(|_| "put failed".to_string())
    }
}

/// Put `raster` on its canvas once the canvas is in the page: a canvas
/// drawn in the same render that mounts it isn't there yet, so the first
/// draw waits a frame or two.
pub async fn put_when_mounted(raster: Raster, canvas_id: String) {
    for _ in 0..24 {
        if raster.put(&canvas_id).is_ok() {
            return;
        }
        gloo_timers::future::TimeoutFuture::new(16).await;
    }
}

/// A disc worked out once, to stamp at every lamp: per device pixel round
/// its centre, how much the disc covers it and how much of its rim.
pub struct Stamp {
    side: u32,
    cells: Vec<(f64, f64)>,
    rim: Option<[f64; 4]>,
}

impl Stamp {
    /// A disc of radius `r` device pixels, with a soft edge; `rim` blends a
    /// lighter ring round it.
    pub fn disc(r: f64, rim: Option<[f64; 4]>) -> Stamp {
        let side = (2.0 * (r + 1.0)).ceil() as u32 | 1;
        let c = side as f64 / 2.0;
        let mut cells = Vec::with_capacity((side * side) as usize);
        for j in 0..side {
            for i in 0..side {
                let d = ((i as f64 + 0.5 - c).powi(2) + (j as f64 + 0.5 - c).powi(2)).sqrt();
                let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                let edge = (1.0 - (r - d).abs()).clamp(0.0, 1.0);
                cells.push((cover, edge));
            }
        }
        Stamp { side, cells, rim }
    }
}

/// A canvas's 2D context.
pub fn context(canvas: &HtmlCanvasElement) -> Option<CanvasRenderingContext2d> {
    canvas.get_context("2d").ok()??.dyn_into().ok()
}

fn to_u8(c: [f64; 4]) -> [u8; 3] {
    [
        c[0].round().clamp(0.0, 255.0) as u8,
        c[1].round().clamp(0.0, 255.0) as u8,
        c[2].round().clamp(0.0, 255.0) as u8,
    ]
}

/// `a` moved toward `b` by `t`.
fn mix(a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3],
    ]
}

/// A fresh element id for a canvas, unique on the page.
pub fn canvas_id(what: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("{what}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

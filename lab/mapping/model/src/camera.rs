//! The canvas camera: fixture units ↔ screen pixels.

use crate::geom::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Where fixture (0, 0) lands on screen, in pixels.
    pub offset: Vec2,
    /// Pixels per fixture unit.
    pub scale: f64,
}

pub const MIN_SCALE: f64 = 0.5;
pub const MAX_SCALE: f64 = 200.0;

impl Camera {
    pub fn to_screen(&self, p: Vec2) -> Vec2 {
        p * self.scale + self.offset
    }

    pub fn to_world(&self, s: Vec2) -> Vec2 {
        (s - self.offset) * (1.0 / self.scale)
    }

    /// Pan by a screen-pixel delta.
    pub fn pan(&mut self, delta: Vec2) {
        self.offset = self.offset + delta;
    }

    /// Zoom by `factor`, keeping the point under `anchor` (screen) still.
    pub fn zoom_at(&mut self, anchor: Vec2, factor: f64) {
        let world = self.to_world(anchor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        self.offset = anchor - world * self.scale;
    }

    /// Frame `rect` in a viewport of `size` pixels, with `margin` pixels
    /// around it.
    pub fn fit(rect: Rect, size: Vec2, margin: f64) -> Camera {
        let w = (size.x - 2.0 * margin).max(1.0);
        let h = (size.y - 2.0 * margin).max(1.0);
        let scale = (w / rect.width().max(1e-6)).min(h / rect.height().max(1e-6));
        let scale = scale.clamp(MIN_SCALE, MAX_SCALE);
        let center = rect.min.lerp(rect.max, 0.5);
        Camera {
            scale,
            offset: size * 0.5 - center * scale,
        }
    }
}

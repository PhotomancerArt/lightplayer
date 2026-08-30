//! Editor camera: pan/zoom over doc space, pure math (ported from the UX
//! spike).

use lpc_mapping::Bounds2d;

// The clamp bounds the EFFECTIVE doc zoom seen through a placement
// (`camera.scale × placement.s`): wide enough that a 0.1-scaled fixture
// still reaches lamp-editing zoom and a large arrangement still fits.
const MIN_SCALE: f32 = 0.02;
const MAX_SCALE: f32 = 64.0;

/// Below this fraction of the framing scale, content reads as too small to
/// work with, so re-framing it earns the camera move. Above it — with the
/// content fully in view — the move would only disturb a view the user can
/// already see (the dive-zoom complaint: a fixture at ~43% of its framing
/// scale was perfectly usable, and diving into it yanked the camera).
const WELL_FRAMED_SCALE_FRACTION: f32 = 0.34;

/// View-pixel slack on the visibility test, so a placement whose edge lands
/// a hair outside by rounding is not called hidden.
const WELL_FRAMED_EDGE_SLACK: f32 = 1.0;

/// The scale [`Camera::fit`] would choose to frame `bounds`.
fn fit_scale(bounds: Bounds2d, viewport_width: f32, viewport_height: f32, padding: f32) -> f32 {
    let usable_width = (viewport_width - 2.0 * padding).max(1.0);
    let usable_height = (viewport_height - 2.0 * padding).max(1.0);
    (usable_width / bounds.width.max(1e-3))
        .min(usable_height / bounds.height.max(1e-3))
        .clamp(MIN_SCALE, MAX_SCALE)
}

/// View transform: `view = doc * scale + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

impl Camera {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        }
    }

    #[must_use]
    pub fn doc_to_view(&self, point: [f32; 2]) -> [f32; 2] {
        [
            point[0] * self.scale + self.x,
            point[1] * self.scale + self.y,
        ]
    }

    #[must_use]
    pub fn view_to_doc(&self, point: [f32; 2]) -> [f32; 2] {
        [
            (point[0] - self.x) / self.scale,
            (point[1] - self.y) / self.scale,
        ]
    }

    pub fn pan(&mut self, view_dx: f32, view_dy: f32) {
        self.x += view_dx;
        self.y += view_dy;
    }

    /// Zoom by `factor` keeping the doc point under `view_point` fixed.
    pub fn zoom_at(&mut self, view_point: [f32; 2], factor: f32) {
        let anchor = self.view_to_doc(view_point);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        self.x = view_point[0] - anchor[0] * self.scale;
        self.y = view_point[1] - anchor[1] * self.scale;
    }

    /// Frame `bounds` inside a `viewport_width` × `viewport_height` view with
    /// `padding` view units on every side, centered.
    pub fn fit(
        &mut self,
        bounds: Bounds2d,
        viewport_width: f32,
        viewport_height: f32,
        padding: f32,
    ) {
        let scale = fit_scale(bounds, viewport_width, viewport_height, padding);
        self.scale = scale;
        self.x = (viewport_width - bounds.width * scale) / 2.0 - bounds.min_x * scale;
        self.y = (viewport_height - bounds.height * scale) / 2.0 - bounds.min_y * scale;
    }

    /// Is `bounds` already framed well enough that re-fitting would only
    /// disturb the view? True when the content is entirely inside the
    /// viewport AND drawn at a workable fraction of its framing scale.
    ///
    /// This is the camera's half of "reveal only when necessary": content
    /// the user can already see and work with is left exactly where it is,
    /// while content that is off-screen, clipped, or too small to work on
    /// still earns the fit.
    #[must_use]
    pub fn frames_well(
        &self,
        bounds: Bounds2d,
        viewport_width: f32,
        viewport_height: f32,
        padding: f32,
    ) -> bool {
        let near = self.doc_to_view([bounds.min_x, bounds.min_y]);
        let far = self.doc_to_view([bounds.min_x + bounds.width, bounds.min_y + bounds.height]);
        let inside = near[0] >= -WELL_FRAMED_EDGE_SLACK
            && near[1] >= -WELL_FRAMED_EDGE_SLACK
            && far[0] <= viewport_width + WELL_FRAMED_EDGE_SLACK
            && far[1] <= viewport_height + WELL_FRAMED_EDGE_SLACK;
        inside
            && self.scale
                >= fit_scale(bounds, viewport_width, viewport_height, padding)
                    * WELL_FRAMED_SCALE_FRACTION
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_and_doc_transforms_round_trip() {
        let camera = Camera {
            x: 40.0,
            y: -12.0,
            scale: 2.5,
        };
        let doc = [123.4, -56.7];
        let back = camera.view_to_doc(camera.doc_to_view(doc));
        assert!((back[0] - doc[0]).abs() < 1e-3);
        assert!((back[1] - doc[1]).abs() < 1e-3);
    }

    #[test]
    fn zoom_at_keeps_the_anchor_fixed() {
        let mut camera = Camera::new();
        camera.pan(50.0, 20.0);
        let view_point = [200.0, 150.0];
        let anchor_before = camera.view_to_doc(view_point);
        camera.zoom_at(view_point, 2.0);
        let anchor_after = camera.view_to_doc(view_point);
        assert!((anchor_before[0] - anchor_after[0]).abs() < 1e-3);
        assert!((anchor_before[1] - anchor_after[1]).abs() < 1e-3);
        assert!((camera.scale - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_clamps_to_range() {
        let mut camera = Camera::new();
        camera.zoom_at([0.0, 0.0], 10000.0);
        assert!((camera.scale - 64.0).abs() < 1e-6);
        camera.zoom_at([0.0, 0.0], 0.000001);
        assert!((camera.scale - 0.02).abs() < 1e-6);
    }

    /// Q7: the clamp reaches lamp-editing zoom on a shrunken placement —
    /// a 0.1-scaled fixture at MAX_SCALE still gives effective doc zoom
    /// 6.4×, and MIN_SCALE zooms out well past a large arrangement.
    #[test]
    fn zoom_range_covers_placement_scaled_fixtures() {
        let mut camera = Camera::new();
        camera.zoom_at([0.0, 0.0], f32::MAX);
        let placement_scale = 0.1;
        assert!(camera.scale * placement_scale > 6.0);
        camera.zoom_at([0.0, 0.0], f32::MIN_POSITIVE);
        assert!(camera.scale <= 0.02);
    }

    #[test]
    fn fit_centers_bounds_in_viewport() {
        let mut camera = Camera::new();
        let bounds = Bounds2d {
            min_x: 100.0,
            min_y: 200.0,
            width: 400.0,
            height: 100.0,
        };
        camera.fit(bounds, 1000.0, 600.0, 50.0);
        // Wide bounds: width limits the scale.
        assert!((camera.scale - 900.0 / 400.0).abs() < 1e-3);
        // The bounds center lands on the viewport center.
        let center = camera.doc_to_view([300.0, 250.0]);
        assert!((center[0] - 500.0).abs() < 1e-2);
        assert!((center[1] - 300.0).abs() < 1e-2);
    }

    const SQUARE: Bounds2d = Bounds2d {
        min_x: 0.0,
        min_y: 0.0,
        width: 100.0,
        height: 100.0,
    };

    /// The camera a fit produces obviously frames its own bounds well —
    /// which is what keeps a settled view from re-fitting itself.
    #[test]
    fn a_fitted_camera_frames_its_bounds_well() {
        let mut camera = Camera::new();
        camera.fit(SQUARE, 600.0, 400.0, 0.0);
        assert!(camera.frames_well(SQUARE, 600.0, 400.0, 0.0));
    }

    /// The reported case: content comfortably visible at a useful size is
    /// left alone — no zoom-to-fill under the user.
    #[test]
    fn visible_content_at_a_workable_size_is_left_alone() {
        // Fit scale here is 4.0 (400/100); at 1.7 the square draws 170px
        // in a 600x400 view, well inside and well over a third of framing.
        let camera = Camera {
            x: 100.0,
            y: 100.0,
            scale: 1.7,
        };
        assert!(camera.frames_well(SQUARE, 600.0, 400.0, 0.0));
    }

    #[test]
    fn content_scrolled_off_screen_still_earns_the_fit() {
        let camera = Camera {
            x: 700.0,
            y: 100.0,
            scale: 1.7,
        };
        assert!(!camera.frames_well(SQUARE, 600.0, 400.0, 0.0));
    }

    #[test]
    fn content_clipped_at_an_edge_still_earns_the_fit() {
        // Overhangs the right edge by 20px.
        let camera = Camera {
            x: 450.0,
            y: 100.0,
            scale: 1.7,
        };
        assert!(!camera.frames_well(SQUARE, 600.0, 400.0, 0.0));
    }

    /// Diving into something tiny is exactly when framing helps: fully
    /// visible, but far too small to work on.
    #[test]
    fn visible_but_tiny_content_still_earns_the_fit() {
        // Framing scale is 4.0; at 0.5 the square draws 50px — fully
        // inside the view, but far under a third of framing, so the
        // refusal here is about SIZE, not visibility.
        let tiny = Camera {
            x: 100.0,
            y: 100.0,
            scale: 0.5,
        };
        assert!(!tiny.frames_well(SQUARE, 600.0, 400.0, 0.0));
        // Just over the threshold (1.36) the same placement is kept.
        let workable = Camera {
            x: 100.0,
            y: 100.0,
            scale: 1.4,
        };
        assert!(workable.frames_well(SQUARE, 600.0, 400.0, 0.0));
    }

    /// Zoomed deep into one corner, the content is not "already framed" —
    /// most of it is outside the viewport.
    #[test]
    fn content_zoomed_past_the_viewport_earns_the_fit() {
        let camera = Camera {
            x: 0.0,
            y: 0.0,
            scale: 20.0,
        };
        assert!(!camera.frames_well(SQUARE, 600.0, 400.0, 0.0));
    }
}

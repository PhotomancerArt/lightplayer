//! Where a document sits in project space: translate ∘ rotate ∘ uniform
//! scale. The canvas renders doc layers inside this transform and routes
//! pointer math through its inverse — the document itself never learns it
//! is placed (the seam is view-layer only).

/// A document's placement in project space (`project = t + R(r) · s · doc`).
///
/// `f64` like the arrange math it replaces; doc points are `f32`, so the
/// `*_f32` helpers convert at the boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Translation in project units.
    pub t: [f64; 2],
    /// Rotation in degrees, counter-clockwise about the doc origin.
    pub r: f64,
    /// Uniform scale.
    pub s: f64,
}

impl Placement {
    pub const IDENTITY: Self = Self {
        t: [0.0, 0.0],
        r: 0.0,
        s: 1.0,
    };

    /// A doc-space point through translate ∘ rotate ∘ scale.
    #[must_use]
    pub fn apply(&self, point: [f64; 2]) -> [f64; 2] {
        let rad = self.r.to_radians();
        let (sin, cos) = rad.sin_cos();
        let sx = point[0] * self.s;
        let sy = point[1] * self.s;
        [
            self.t[0] + sx * cos - sy * sin,
            self.t[1] + sx * sin + sy * cos,
        ]
    }

    /// The inverse of [`Self::apply`]: a project-space point into doc space.
    #[must_use]
    pub fn inverse(&self, point: [f64; 2]) -> [f64; 2] {
        let rad = self.r.to_radians();
        let (sin, cos) = rad.sin_cos();
        let dx = point[0] - self.t[0];
        let dy = point[1] - self.t[1];
        let scale = self.s.max(1e-9);
        [
            (dx * cos + dy * sin) / scale,
            (-dx * sin + dy * cos) / scale,
        ]
    }

    /// [`Self::apply`] over `f32` doc points.
    #[must_use]
    pub fn apply_f32(&self, point: [f32; 2]) -> [f32; 2] {
        let out = self.apply([f64::from(point[0]), f64::from(point[1])]);
        [out[0] as f32, out[1] as f32]
    }

    /// [`Self::inverse`] over `f32` project points.
    #[must_use]
    pub fn inverse_f32(&self, point: [f32; 2]) -> [f32; 2] {
        let out = self.inverse([f64::from(point[0]), f64::from(point[1])]);
        [out[0] as f32, out[1] as f32]
    }

    /// The scale as `f32`, for folding into screen-constant sizing
    /// (`/(camera.scale * placement.scale_f32())`).
    #[must_use]
    pub fn scale_f32(&self) -> f32 {
        self.s as f32
    }

    /// The four corners of an `[x, y, w, h]` doc-space bounds rect in
    /// project space (fit math over placed documents).
    #[must_use]
    pub fn corners(&self, bounds: [f64; 4]) -> [[f64; 2]; 4] {
        let [bx, by, bw, bh] = bounds;
        [
            self.apply([bx, by]),
            self.apply([bx + bw, by]),
            self.apply([bx, by + bh]),
            self.apply([bx + bw, by + bh]),
        ]
    }

    /// The SVG group transform rendering doc space at this placement.
    #[must_use]
    pub fn svg_transform(&self) -> String {
        format!(
            "translate({} {}) rotate({}) scale({})",
            self.t[0], self.t[1], self.r, self.s
        )
    }
}

impl Default for Placement {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_a_no_op() {
        let placement = Placement::IDENTITY;
        let point = [123.4, -56.7];
        assert_eq!(placement.apply(point), point);
        assert_eq!(placement.inverse(point), point);
        assert_eq!(
            placement.svg_transform(),
            "translate(0 0) rotate(0) scale(1)"
        );
    }

    #[test]
    fn apply_then_inverse_round_trips_under_rotation_and_scale() {
        let placement = Placement {
            t: [40.0, -12.0],
            r: 37.5,
            s: 0.35,
        };
        let point = [123.4, -56.7];
        let back = placement.inverse(placement.apply(point));
        assert!((back[0] - point[0]).abs() < 1e-9);
        assert!((back[1] - point[1]).abs() < 1e-9);
    }

    #[test]
    fn corners_rotate_about_the_group_origin() {
        let placement = Placement {
            t: [10.0, 0.0],
            r: 90.0,
            s: 1.0,
        };
        let points = placement.corners([0.0, 0.0, 4.0, 2.0]);
        // (4, 0) rotates to (0, 4), then translates by (10, 0).
        assert!((points[1][0] - 10.0).abs() < 1e-9);
        assert!((points[1][1] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn pointer_math_composes_camera_then_placement_inverse() {
        // The canvas's event → doc pipeline: view → project (camera), then
        // project → doc (placement inverse). A doc point rendered through
        // placement ∘ camera must come back exactly.
        let camera = crate::editor_core::camera::Camera {
            x: 80.0,
            y: 30.0,
            scale: 2.0,
        };
        let placement = Placement {
            t: [15.0, -4.0],
            r: 15.0,
            s: 0.1,
        };
        let doc_point = [42.0_f32, 17.0_f32];
        let project = placement.apply_f32(doc_point);
        let view = camera.doc_to_view(project);
        let back = placement.inverse_f32(camera.view_to_doc(view));
        assert!((back[0] - doc_point[0]).abs() < 1e-3);
        assert!((back[1] - doc_point[1]).abs() < 1e-3);
    }
}

//! Document framing: what "zoom to fit" means for a mapping document —
//! the authored canvas when there is one (the display mode renders exactly
//! that region, so the view ⇄ edit flip lands on the same framing), else
//! the resolved lamp bounds, padded by the display-mode inset.

use lpc_mapping::{Bounds2d, Map2dDoc, bounds_of_points, resolve};

/// The doc-space region a fit should frame.
#[must_use]
pub fn doc_fit_bounds(doc: &Map2dDoc) -> Option<Bounds2d> {
    doc.canvas_bounds().or_else(|| {
        resolve(doc)
            .ok()
            .and_then(|resolved| bounds_of_points(&resolved.positions()))
    })
}

/// Match the display-mode framing (`ux-map-inset`, 5.5% inset) so flipping
/// view ⇄ edit keeps the fixture at the same on-screen size: pad by 5.5%
/// of the limiting viewport dimension.
#[must_use]
pub fn display_inset_padding(bounds: Bounds2d, viewport_width: f32, viewport_height: f32) -> f32 {
    let width_limited =
        bounds.width / bounds.height.max(1e-6) >= viewport_width / viewport_height.max(1.0);
    0.055
        * if width_limited {
            viewport_width
        } else {
            viewport_height
        }
}

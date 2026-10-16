//! A render product: a picture you can sample at a point and a time — what
//! a render source publishes on its visual channel, and what a fixture
//! samples at its lamps (the product's `VisualProduct`, D55).
//!
//! In the product it is a texture a shader renders; here it is the
//! source's stand-in [`Pattern`](crate::pattern::Pattern). A fixture
//! samples it in its own box, as the unit square: `(0, 0)` its top-left
//! corner, `(1, 1)` its bottom-right (`Fixture::uv`).

use crate::geom::{Rect, Vec2};
use crate::node::NodeId;
use crate::pattern::Pattern;
use crate::project::Project;
use crate::rgb::Rgb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderProduct {
    /// The render source that publishes it.
    pub source: NodeId,
    pub pattern: Pattern,
}

impl RenderProduct {
    /// Its colour at `uv` (the unit square) at `time` seconds.
    pub fn sample(&self, uv: Vec2, time: f64) -> Rgb {
        self.pattern.sample(uv, time)
    }
}

impl Project {
    /// The render product a fixture samples: its visual channel's, from
    /// the nearest writer (`bindings.rs`). `None` when nothing writes it.
    pub fn render_product(&self, fixture: &NodeId) -> Option<RenderProduct> {
        let source = self.visual_source(fixture)?;
        let pattern = self.node(&source)?.source()?.pattern;
        Some(RenderProduct { source, pattern })
    }

    /// A fixture's render product as a coarse grid of cells over its box,
    /// in world units: the picture its lamps sample, for the canvas to show
    /// faintly behind them.
    pub fn render_preview(&self, fixture: &NodeId, cells: u32, time: f64) -> Vec<(Rect, Rgb)> {
        let (Some(product), Some(rect)) = (self.render_product(fixture), self.node_rect(fixture))
        else {
            return Vec::new();
        };
        let n = cells.max(1);
        let size = Vec2::new(rect.width() / n as f64, rect.height() / n as f64);
        let mut out = Vec::with_capacity((n * n) as usize);
        for j in 0..n {
            for i in 0..n {
                let min = rect.min + Vec2::new(size.x * i as f64, size.y * j as f64);
                let uv = Vec2::new((i as f64 + 0.5) / n as f64, (j as f64 + 0.5) / n as f64);
                out.push((Rect::new(min, min + size), product.sample(uv, time)));
            }
        }
        out
    }
}

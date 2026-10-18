//! A picture: one frame of a render source's render product, as the board
//! (or the preview host) sent it — `width` × `height` pixels, red, green,
//! blue, row by row from the top-left (the product's `Srgb8` preview).
//!
//! The Graph shows it twice (plan D11): on the source's own card, and
//! faintly behind the lamps of a fixture that samples it, where the
//! product's fit puts it ([`picture_rect`]).

use std::sync::Arc;

use crate::domain::geom::{Rect, Vec2};
use crate::domain::rgb::Rgb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// `width × height × 3` bytes.
    pub rgb: Arc<[u8]>,
}

impl Picture {
    /// A picture from its bytes; `None` when they aren't `width × height`
    /// pixels.
    pub fn new(width: u32, height: u32, rgb: Arc<[u8]>) -> Option<Picture> {
        (width > 0 && height > 0 && rgb.len() == (width * height * 3) as usize).then_some(Picture {
            width,
            height,
            rgb,
        })
    }

    /// A picture made by sampling `colour` at each pixel's centre, `(0, 0)`
    /// the top-left corner and `(1, 1)` the bottom-right: the lab's
    /// simulated board drawing a stand-in pattern.
    pub fn sampled(width: u32, height: u32, colour: impl Fn(Vec2) -> Rgb) -> Picture {
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for j in 0..height {
            for i in 0..width {
                let uv = Vec2::new(
                    (i as f64 + 0.5) / width as f64,
                    (j as f64 + 0.5) / height as f64,
                );
                let c = colour(uv);
                rgb.extend([c.r, c.g, c.b]);
            }
        }
        Picture {
            width,
            height,
            rgb: rgb.into(),
        }
    }

    /// The pixel at column `x`, row `y`.
    pub fn at(&self, x: u32, y: u32) -> Rgb {
        let i = ((y.min(self.height - 1) * self.width + x.min(self.width - 1)) * 3) as usize;
        Rgb::new(self.rgb[i], self.rgb[i + 1], self.rgb[i + 2])
    }
}

/// Where a `width` × `height` picture lies in a fixture whose box is
/// `frame`, in the fixture's own units: the product's fit, inverted.
///
/// The product fits a fixture's box into its picture keeping the box's
/// shape, centred on the short way (`lpc-mapping`'s `fit_points`): the box
/// spans the picture the long way and sits in its middle the other. So the
/// picture spans the box one way and runs past it, evenly, the other; a
/// lamp sits on the colour it samples.
pub fn picture_rect(frame: Rect, width: u32, height: u32) -> Rect {
    let (bw, bh) = (frame.width(), frame.height());
    if bw <= 0.0 || bh <= 0.0 || width == 0 || height == 0 {
        return frame;
    }
    let picture = width as f64 / height as f64;
    let size = if bw / bh >= picture {
        Vec2::new(bw, bw / picture)
    } else {
        Vec2::new(bh * picture, bh)
    };
    let centre = Vec2::new(frame.min.x + bw / 2.0, frame.min.y + bh / 2.0);
    let half = Vec2::new(size.x / 2.0, size.y / 2.0);
    Rect::new(centre - half, centre + half)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square box in a square picture is the picture; a box twice as wide
    /// as tall in a square picture spans it the long way and sits in its
    /// middle, so the picture runs past the box above and below.
    #[test]
    fn a_picture_lies_where_the_product_fits_the_box() {
        let square = Rect::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        assert_eq!(picture_rect(square, 32, 32), square);
        let wide = Rect::new(Vec2::new(10.0, 0.0), Vec2::new(210.0, 100.0));
        assert_eq!(
            picture_rect(wide, 32, 32),
            Rect::new(Vec2::new(10.0, -50.0), Vec2::new(210.0, 150.0))
        );
    }
}

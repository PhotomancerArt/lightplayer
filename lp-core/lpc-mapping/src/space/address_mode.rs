//! What one axis does past a bounded product's extent: GPU sampler address
//! modes (OpenGL, Vulkan, WebGPU).

/// One axis of an outside rule (theory D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    /// The rule's solid colour; the product is not evaluated there (D18).
    Solid,
    /// Hold the edge (clamp to edge).
    Clamp,
    /// Tile (repeat).
    Wrap,
    /// Tile, every other copy reflected (mirrored repeat).
    Mirror,
}

impl AddressMode {
    /// Fold a Q16.16 coordinate into `min..=max` the way this mode does, or
    /// `None` for [`Self::Solid`] when it is outside. A coordinate already
    /// inside is returned as it is (the range is inclusive).
    #[must_use]
    pub(crate) fn resolve(self, value: i32, min: i32, max: i32) -> Option<i32> {
        if (min..=max).contains(&value) {
            return Some(value);
        }
        let span = i64::from(max) - i64::from(min);
        if span <= 0 {
            return match self {
                Self::Solid => None,
                Self::Clamp | Self::Wrap | Self::Mirror => Some(min),
            };
        }
        let offset = i64::from(value) - i64::from(min);
        let folded = match self {
            Self::Solid => return None,
            Self::Clamp => offset.clamp(0, span),
            Self::Wrap => offset.rem_euclid(span),
            Self::Mirror => {
                let phase = offset.rem_euclid(2 * span);
                if phase <= span {
                    phase
                } else {
                    2 * span - phase
                }
            }
        };
        Some((i64::from(min) + folded) as i32)
    }
}

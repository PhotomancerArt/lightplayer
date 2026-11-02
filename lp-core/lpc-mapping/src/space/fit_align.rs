//! Where a fitted product box sits in the fixture's box: SVG's nine
//! `xMin…xMax` × `yMin…yMax` alignments.

/// One axis of an alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignAxis {
    /// The product's low edge on the box's low edge (x: left; y: top, as
    /// the fixture's y runs down — SVG's `yMin`).
    Min,
    /// Centred.
    #[default]
    Mid,
    /// The product's high edge on the box's high edge (x: right; y: bottom).
    Max,
}

impl AlignAxis {
    /// The offset of a placed span of `placed` length inside a box span of
    /// `size`, from the box's low edge.
    #[must_use]
    pub(crate) fn offset(self, size: f64, placed: f64) -> f64 {
        match self {
            Self::Min => 0.0,
            Self::Mid => (size - placed) / 2.0,
            Self::Max => size - placed,
        }
    }
}

/// The fit's alignment (theory D16): SVG `preserveAspectRatio`'s nine,
/// `xMidYMid` by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FitAlign {
    pub x: AlignAxis,
    pub y: AlignAxis,
}

impl FitAlign {
    /// Centred on both axes (SVG `xMidYMid`).
    pub const CENTRE: Self = Self {
        x: AlignAxis::Mid,
        y: AlignAxis::Mid,
    };

    #[must_use]
    pub const fn new(x: AlignAxis, y: AlignAxis) -> Self {
        Self { x, y }
    }
}

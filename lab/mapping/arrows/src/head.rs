//! An arrowhead: what an arrow's end looks like.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadShape {
    /// A filled triangle; the line stops at its base.
    Triangle,
    /// An open "V", stroked; the line runs to its tip.
    Chevron,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Head {
    pub shape: HeadShape,
    /// From base to tip, in the caller's units.
    pub length: f64,
    /// Across the base.
    pub width: f64,
}

impl Head {
    pub fn triangle(length: f64, width: f64) -> Head {
        Head {
            shape: HeadShape::Triangle,
            length,
            width,
        }
    }

    pub fn chevron(length: f64, width: f64) -> Head {
        Head {
            shape: HeadShape::Chevron,
            length,
            width,
        }
    }

    /// How far the line is cut back from the tip so it doesn't poke
    /// through: a triangle's length, nothing for a chevron.
    pub fn inset(&self) -> f64 {
        match self.shape {
            HeadShape::Triangle => self.length,
            HeadShape::Chevron => 0.0,
        }
    }
}

impl Default for Head {
    /// The lab's edge head: a triangle 7 long and 7 wide.
    fn default() -> Self {
        Head::triangle(7.0, 7.0)
    }
}

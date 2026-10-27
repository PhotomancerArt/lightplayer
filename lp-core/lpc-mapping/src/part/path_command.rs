//! One command of a path, in SVG path data's words.
//!
//! M1 builds Move and Line: a line from the point before. A Move after the
//! first starts a new **subpath**; the stretch it skips is the **jumper**
//! (wire where a strip is cut and jumpered across, lit by nothing). Close
//! (`Z`), curves and arcs come later; they are SVG's commands too, so adding
//! them is not a new idea.

/// One path command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    /// `M x y`: go to a point without drawing. The first command of every
    /// path; any later one is a jumper.
    Move([f32; 2]),
    /// `L x y`: a straight line to a point.
    Line([f32; 2]),
}

impl PathCommand {
    /// The point it ends at.
    #[must_use]
    pub fn point(self) -> [f32; 2] {
        match self {
            PathCommand::Move(p) | PathCommand::Line(p) => p,
        }
    }

    #[must_use]
    pub fn is_move(self) -> bool {
        matches!(self, PathCommand::Move(_))
    }
}

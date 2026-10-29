//! A path: one strip's route, in SVG path data's words (names.html "Path").
//!
//! Commands move (`M`) and draw lines (`L`); the stretch between moves is a
//! subpath, and a move inside a path is the jumper. Lamps spread over the
//! drawn subpaths only, so the path keeps one count and one pitch. Its lamps
//! are today's [`PathShape`]'s, bit for bit: the points are every command's
//! point in order, and each move after the first makes the line ending at
//! it a jumper (today's `gaps`).

use alloc::vec::Vec;

use crate::map2d_doc::{PathAlign, PathShape};
use crate::part::path_command::PathCommand;
use crate::part::path_split::PathSplit;

/// A run of lamps along a route of moves and lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Move first, then lines and moves.
    pub commands: Vec<PathCommand>,
    /// Lamps along the drawn subpaths.
    pub count: u32,
    /// Lamp 0 at the last point instead of the first: the path's own
    /// direction ("drawn from"), a layout fact, not a patch one.
    pub reversed: bool,
    /// Stroke alignment, render-only.
    pub align: PathAlign,
    /// One segment, or one per subpath.
    pub split: PathSplit,
}

impl Path {
    /// A path through `points` (a move to the first, lines to the rest),
    /// with `count` lamps.
    #[must_use]
    pub fn through(points: &[[f32; 2]], count: u32) -> Self {
        let commands = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    PathCommand::Move(*p)
                } else {
                    PathCommand::Line(*p)
                }
            })
            .collect();
        Self::of(commands, count)
    }

    /// A path of these commands, with `count` lamps, drawn forward, one
    /// segment.
    #[must_use]
    pub fn of(commands: Vec<PathCommand>, count: u32) -> Self {
        Self {
            commands,
            count,
            reversed: false,
            align: PathAlign::On,
            split: PathSplit::One,
        }
    }

    /// How many subpaths it draws: one per move.
    #[must_use]
    pub fn subpath_count(&self) -> usize {
        self.commands.iter().filter(|c| c.is_move()).count()
    }

    /// Today's form of the same path, whose lamps it has: every command's
    /// point in order, and the line ending at each later move a jumper.
    /// `Err` names why it isn't a path: it must start with a move.
    pub fn to_path_shape(&self) -> Result<PathShape, &'static str> {
        match self.commands.first() {
            Some(PathCommand::Move(_)) => {}
            Some(PathCommand::Line(_)) => return Err("path must start with a move"),
            None => return Err("path has no commands"),
        }
        let points = self.commands.iter().map(|c| c.point()).collect();
        // Line `i` runs from point `i` to point `i + 1`: a move at
        // command `i + 1` makes it a jumper.
        let gaps = self
            .commands
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, c)| c.is_move())
            .map(|(i, _)| i as u32 - 1)
            .collect();
        Ok(PathShape {
            points,
            count: self.count,
            reversed: self.reversed,
            gaps,
            align: self.align,
        })
    }
}

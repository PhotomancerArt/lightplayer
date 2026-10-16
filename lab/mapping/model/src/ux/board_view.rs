//! The board as the editor shows it: the simulated board's frame for the
//! current selection, or for a drawing in progress (`board.rs` is the
//! board itself).

use crate::domain::board::BoardFrame;
use crate::domain::fixture_overrides::FixtureOverrides;
use crate::domain::output_overrides::OutputOverrides;
use crate::domain::rgb::Rgb;
use crate::ux::editor::{Editor, Gesture, Tool};

impl Editor {
    /// What the simulated board shows at `time` seconds.
    pub fn board_frame(&self, time: f64) -> BoardFrame {
        if self.drawing() {
            self.draft_frame(time)
        } else {
            self.project.board_frame(&self.selection, time)
        }
    }

    /// Whether the board's pixels move on their own right now (a selection's
    /// pattern, a drawing in progress), so the page knows to keep ticking.
    /// The stand-in patterns hold still (`pattern.rs`).
    pub fn board_animates(&self) -> bool {
        !self.selection.is_empty() || self.drawing()
    }

    /// A circle being dragged out, or a path being clicked out.
    pub fn drawing(&self) -> bool {
        matches!(self.gesture, Gesture::Drawing { .. })
            || (self.tool == Tool::Path && !self.path_draft.is_empty())
    }

    /// The draft as if it were already drawn, under its fixture's own
    /// rules: only its lamps lit (a fixture override), on the pixels the
    /// board has now. A draft that starts outside every fixture lands
    /// nowhere, so nothing lights.
    fn draft_frame(&self, time: f64) -> BoardFrame {
        let nowhere = || {
            self.project.evaluate(
                FixtureOverrides::Drawing(None),
                OutputOverrides::default(),
                time,
            )
        };
        let Some((fixture, kind)) = self.draft_into() else {
            return nowhere();
        };
        let mut p = self.project.clone();
        let Some(draft) = p.add(&fixture, kind) else {
            return nowhere();
        };
        let mut frame = p.evaluate(
            FixtureOverrides::Drawing(Some(draft)),
            OutputOverrides::default(),
            time,
        );
        // The board as it is: the draft lights only pixels it already has.
        let now = self.project.wiring();
        for (port, (_, _, pixels)) in frame.ports.iter_mut().zip(&now.ports) {
            port.pixels.resize(*pixels, Rgb::BLACK);
        }
        frame
    }
}

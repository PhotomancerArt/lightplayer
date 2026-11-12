//! The board as the editor shows it: the simulated board's frame for the
//! current selection, or for a drawing in progress (`board.rs` is the
//! board itself).

use crate::domain::board::BoardFrame;
use crate::domain::fixture_overrides::FixtureOverrides;
use crate::domain::output_overrides::OutputOverrides;
use crate::domain::part_ref::PartRef;
use crate::domain::project::Project;
use crate::domain::rgb::Rgb;
use crate::ux::editor::{Editor, Gesture, Tool};

impl Editor {
    /// What the simulated board shows at `time` seconds.
    pub fn board_frame(&self, time: f64) -> BoardFrame {
        if self.drawing() {
            self.draft_frame(time)
        } else if let Some(frame) = self.armed_frame(time) {
            frame
        } else {
            self.project.board_frame(&self.selection, time)
        }
    }

    /// A circle being dragged out, or a path being clicked out.
    pub fn drawing(&self) -> bool {
        matches!(self.gesture, Gesture::Drawing { .. })
            || (self.tool == Tool::Path && !self.path_draft.is_empty())
    }

    /// The draft as if it were already drawn: the project with it added,
    /// and the part it became there. `None` while nothing is being drawn,
    /// and for a draft that starts outside every fixture (it lands
    /// nowhere). The simulated board's preview and the real board's
    /// (Studio's `graph_highlight.rs`) both start here.
    pub fn drafted(&self) -> Option<(Project, PartRef)> {
        if !self.drawing() {
            return None;
        }
        let (fixture, kind) = self.draft_into()?;
        let mut p = self.project.clone();
        let draft = p.add(&fixture, kind)?;
        Some((p, draft))
    }

    /// The draft as if it were already drawn, under its fixture's own
    /// rules: its lamps in the selection's pattern and every other lamp
    /// dimmed (a fixture override), on the pixels the board has now. A
    /// draft that starts outside every fixture lands nowhere, so nothing
    /// is shown in the pattern.
    fn draft_frame(&self, time: f64) -> BoardFrame {
        let Some((p, draft)) = self.drafted() else {
            return self.project.evaluate(
                FixtureOverrides::Drawing(None),
                OutputOverrides::default(),
                time,
            );
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

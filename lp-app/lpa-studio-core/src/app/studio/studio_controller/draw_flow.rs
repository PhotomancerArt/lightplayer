//! Drawing on the picture, on the studio controller: which live pictures are
//! drawable, a reported stroke into paced touch writes, and the picture's
//! pace while one is live (plan `drawing-on-the-picture`, D11–D13, Q5, Q8,
//! Q14).
//!
//! Every drawable picture is the open session's own (the editor's lens):
//! the connected board's card while its panel is up, and the root module's
//! hero, which Play's banner and the editor both draw. A watched card, a
//! library card, a session that dropped and a project with no `touches`
//! reader are not drawable. The session may be a board or a sim: both are a
//! live engine taking the same panel writes.
//!
//! The writes are ordinary momentary [`crate::PanelWriteOp`]s built here and
//! run as actions in the actor's batch, through the lens session like a
//! knob's — never offered (a stroke is not a verb) and never echoed as a
//! held value.

use core::time::Duration;

use lpa_devices::DeviceId;

use crate::{
    ConnectPhase, ControllerId, ProjectController, StudioController, UiAction, UiDrawSurface,
    UiDrawTarget, UiDrawable, UiStroke,
};

impl StudioController {
    /// Where the open session's touches go, while its picture can be drawn
    /// on: the session is live (not dropped or held), its picture has lamps,
    /// and a `touches` reader is loaded.
    fn lens_draw_target(&self) -> Option<UiDrawTarget> {
        if !self.has_lightplayer_state() || self.lens_hold.is_some() {
            return None;
        }
        let (frame, _) = self.project.lens_published_frame()?;
        frame.display_layout.as_ref()?;
        self.project.touches_draw_target()
    }

    /// The connected board's card picture, while its panel is up on the
    /// home page and the session's picture is drawable.
    pub(super) fn board_drawable(&self) -> Option<(DeviceId, UiDrawable)> {
        let connected = self.connected()?;
        if connected.phase != ConnectPhase::Open || !self.connected_shows_on_its_card() {
            return None;
        }
        let target = self.lens_draw_target()?;
        Some((
            connected.device,
            UiDrawable {
                surface: UiDrawSurface::Board(connected.device),
                target,
            },
        ))
    }

    /// The open project's picture (the root module's hero: the editor's
    /// and Play's), while the session's picture is drawable.
    pub(super) fn project_drawable(&self) -> Option<UiDrawable> {
        Some(UiDrawable {
            surface: UiDrawSurface::Project,
            target: self.lens_draw_target()?,
        })
    }

    /// Where a stroke on `surface` writes, if that picture is drawable now.
    fn surface_draw_target(&self, surface: UiDrawSurface) -> Option<UiDrawTarget> {
        match surface {
            UiDrawSurface::Board(device) => self
                .board_drawable()
                .filter(|(board, _)| *board == device)
                .map(|(_, drawable)| drawable.target),
            UiDrawSurface::Project => self.lens_draw_target(),
        }
    }

    /// The web reported the fingers on a picture (`StudioCommand::Stroke`):
    /// fold them into the stroke and return the write to run now, if one is
    /// due. An empty report releases the stroke (its one empty-set write).
    /// A report on a picture that is not drawable (any more) drops the
    /// stroke without a write: the board lets go within the ttl.
    pub fn apply_stroke(&mut self, stroke: UiStroke) -> Option<UiAction> {
        let now = (self.now_secs)();
        if stroke.pointers.is_empty() {
            self.stroke_frame = None;
            return self.strokes.release(now).map(stroke_action);
        }
        let Some(target) = self.surface_draw_target(stroke.surface) else {
            self.strokes.cancel();
            self.stroke_frame = None;
            return None;
        };
        // The lamp layout is read once per stroke: the picture's geometry
        // does not move under a finger, and a frame is composed per read.
        if self.strokes.surface().is_none() || self.stroke_frame.is_none() {
            self.stroke_frame = self
                .project
                .lens_published_frame()
                .and_then(|(frame, _)| frame.display_layout)
                .and_then(|layout| {
                    let lpc_model::ControlDisplayLayout::Layout2d(layout) = layout.as_ref();
                    crate::LampPictureFrame::of(layout)
                });
        }
        let frame = self.stroke_frame.as_ref()?;
        self.strokes
            .sample(now, stroke.surface, &target, frame, &stroke.pointers)
            .map(stroke_action)
    }

    /// The write the live stroke owes with no new sample (the newest
    /// positions once the send interval has passed, or a resting finger's
    /// renewal), checked on every batch. A stroke whose picture stopped
    /// being drawable is dropped without a write.
    pub fn due_stroke_write(&mut self) -> Option<UiAction> {
        let surface = self.strokes.surface()?;
        if self.surface_draw_target(surface).is_none() {
            self.strokes.cancel();
            self.stroke_frame = None;
            return None;
        }
        let now = (self.now_secs)();
        self.strokes.due(now).map(stroke_action)
    }

    /// How long until the live stroke owes a write: the refresh timer must
    /// fire by then, or a resting finger would expire on the board.
    pub(super) fn stroke_due_in(&self, now: f64) -> Option<Duration> {
        self.strokes.next_due_in(now)
    }

    /// The picture's pace while a stroke is live and for a second after:
    /// the editor's lens gap (Q8). It only ever tightens the lens's own gap,
    /// which is what a connected board's picture reads at (CD8) — over USB,
    /// Bluetooth (Play's idle minute included) and the relay alike (Q14).
    pub(super) fn stroke_feed_gap(&self, now: f64) -> Option<Duration> {
        self.strokes.feed_gap(now)
    }
}

/// A stroke's write as an action: built by core, run like a knob's.
fn stroke_action(op: crate::PanelWriteOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op).with_label("Draw")
}

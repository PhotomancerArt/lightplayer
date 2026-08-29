//! One preview slot's output-frame state: what the worker's
//! `preview_output_frame` answers add up to.
//!
//! Target-neutral like the rest of the slot vocabulary, so the decisions —
//! what to ask for next, what counts as a new frame — are unit-testable
//! without a browser or a worker.
//!
//! # What this exists to guarantee
//!
//! - **The engine owns "control-first".** Whether a project leads with lamps
//!   is answered where the graph is (does the ROOT scope resolve
//!   `control.out`?) and delivered on the frame; nothing here re-derives it
//!   from a manifest. Until the first answer lands the feed asks; a `false`
//!   answer stops the asking for good, which is how a shader-only slot pays
//!   nothing per frame.
//! - **Revision-only change detection.** A new frame is one whose `revision`
//!   moved — the published buffer's `changed_at`. A repeat leaves the frame,
//!   its bytes `Rc`, and [`PreviewOutputFeed::frame_revision`] untouched, so
//!   a consumer polling the revision repaints only on real change.
//! - **The LampView `Rc` contract.** Every genuinely new frame carries a
//!   FRESH `bytes` `Rc` (the renderer's repaint key) while the display layout
//!   keeps a STABLE one across frames whose geometry did not move — both
//!   kept by the per-output folding in [`OutputFrameCache`].
//! - **Every output joins the picture.** A project can drive several
//!   outputs, and the slot composes ALL of them
//!   ([`OutputFrameCache::composed_frame`]) — one buffer, every wire's
//!   lamps at their own offsets. The feed used to latch the FIRST output
//!   that published, which is how the small dome's second box (2,975
//!   lamps) never appeared on a preview card.
//!
//! The device card feed (`crate::app::runtime_pool::card_feed`) keeps the
//! same guarantees for a session at the far end of a serial link, where the
//! read is a host-driven pull with its own pacing and offline story. This one
//! rides a frame the preview host already schedules, so it owns neither.

use std::rc::Rc;

use lpc_wire::{ControlDisplayLayoutRead, OutputFrameEntry};

use crate::UiControlProductPreview;
use crate::app::project::output_frame_cache::OutputFrameCache;

/// Output-frame state for one [`super::PreviewSlotHandle`].
#[derive(Debug, Default)]
pub struct PreviewOutputFeed {
    /// The engine's answer to "does the root scope resolve `control.out`?",
    /// `None` until the first frame answers.
    control_first: Option<bool>,
    /// The per-output half: every output's newest frame and geometry, plus
    /// the read gate — runtime-scoped claims, reset on invalidate.
    outputs: OutputFrameCache,
    /// The newest COMPOSED picture, as the renderer consumes it. Held
    /// outside [`Self::outputs`] so it survives `invalidate_runtime` (the
    /// slot's last picture) while the claims behind it reset.
    frame: Option<UiControlProductPreview>,
    /// Bumped once per genuinely new frame; the consumer's cheap poll.
    frame_revision: u64,
}

impl PreviewOutputFeed {
    /// The engine's control-first verdict, `None` until the first answer.
    pub fn control_first(&self) -> Option<bool> {
        self.control_first
    }

    /// The newest composed output frame, if one has landed.
    pub fn frame(&self) -> Option<&UiControlProductPreview> {
        self.frame.as_ref()
    }

    /// Bumped on every genuinely new frame — poll this to re-read cheaply.
    pub fn frame_revision(&self) -> u64 {
        self.frame_revision
    }

    /// What the next preview frame should ask for: `None` once the engine
    /// has said this project is not control-first (no output traffic at
    /// all), otherwise the shared multi-output geometry gate
    /// ([`OutputFrameCache::display_layout_read`]) — `Always` while any
    /// output still lacks a layout, `IfChanged` while every layout stands,
    /// `None` once the engine refused them all.
    pub fn next_read(&self) -> Option<ControlDisplayLayoutRead> {
        if self.control_first == Some(false) {
            return None;
        }
        Some(self.outputs.display_layout_read())
    }

    /// Fold one `preview_output_frame` answer into the feed and recompose
    /// the picture.
    pub fn apply(&mut self, control_first: bool, outputs: &[OutputFrameEntry]) {
        self.control_first = Some(control_first);
        let before = self.outputs.frames_seen();
        self.outputs.apply(outputs);
        if self.outputs.frames_seen() != before {
            self.frame_revision += 1;
        }
        let nodes: Vec<lpc_model::NodeId> = self.outputs.outputs().collect();
        if let Some(composed) = self.outputs.composed_frame(&nodes) {
            self.frame = Some(composed);
        }
    }

    /// Drop everything that was a claim about the slot's RUNTIME — the
    /// per-output state, geometry and refusals included — when that runtime
    /// goes away (eviction, worker recycle). The last composed frame stays
    /// on screen and the control-first verdict stands: both are facts about
    /// the project, which a re-lease redeploys unchanged.
    pub fn invalidate_runtime(&mut self) {
        self.outputs = OutputFrameCache::default();
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ColorOrder, ControlDisplayLayout, ControlLamp2d, ControlLayout2d, ControlSampleEncoding,
        ControlSampleLayout, ControlSampleSpan, NodeId, Revision,
    };
    use lpc_wire::{ControlDisplayLayoutProbeResult, WireChannelSampleFormat};

    use super::*;

    /// A shader-only slot asks exactly once and never again: the engine's
    /// `false` is the whole answer, so those cards carry no frame traffic.
    #[test]
    fn a_project_that_is_not_control_first_stops_the_asking() {
        let mut feed = PreviewOutputFeed::default();
        assert_eq!(feed.next_read(), Some(ControlDisplayLayoutRead::Always));

        feed.apply(false, &[]);

        assert_eq!(feed.control_first(), Some(false));
        assert_eq!(feed.next_read(), None);
        assert!(feed.frame().is_none());
    }

    #[test]
    fn a_moved_revision_is_a_new_frame_with_fresh_bytes() {
        let mut feed = PreviewOutputFeed::default();

        feed.apply(true, &[entry(4, 1, vec![1, 0, 2, 0, 3, 0])]);
        let first = Rc::clone(&feed.frame().expect("first frame").bytes);
        assert_eq!(feed.frame_revision(), 1);

        feed.apply(true, &[entry(4, 2, vec![9, 0, 8, 0, 7, 0])]);

        let frame = feed.frame().expect("second frame");
        assert_eq!(frame.revision, 2);
        assert_eq!(
            frame.extent,
            lpc_model::ControlExtent::new(1, 3),
            "one RGB lamp is three samples"
        );
        assert!(
            !Rc::ptr_eq(&first, &frame.bytes),
            "every new frame must carry a fresh bytes Rc — the renderer's repaint key"
        );
        assert_eq!(feed.frame_revision(), 2);
    }

    #[test]
    fn a_repeated_revision_is_not_a_new_frame() {
        let mut feed = PreviewOutputFeed::default();
        feed.apply(true, &[entry(4, 7, vec![1, 0, 2, 0, 3, 0])]);
        let bytes = Rc::clone(&feed.frame().expect("frame").bytes);

        // Same revision, different bytes: the engine did not publish, so this
        // is the same picture.
        feed.apply(true, &[entry(4, 7, vec![4, 0, 5, 0, 6, 0])]);

        assert!(Rc::ptr_eq(&bytes, &feed.frame().expect("frame").bytes));
        assert_eq!(feed.frame_revision(), 1);
    }

    #[test]
    fn geometry_travels_once_and_keeps_its_rc_while_unchanged() {
        let mut feed = PreviewOutputFeed::default();
        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));

        feed.apply(true, &[first]);

        assert_eq!(
            feed.next_read(),
            Some(ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(Revision::new(11)),
            })
        );
        let geometry = Rc::clone(
            feed.frame()
                .expect("frame")
                .display_layout
                .as_ref()
                .expect("layout"),
        );

        let mut second = entry(4, 2, vec![9, 0, 8, 0, 7, 0]);
        second.display_layout = ControlDisplayLayoutProbeResult::Unchanged {
            revision: Revision::new(11),
        };
        feed.apply(true, &[second]);

        assert!(
            Rc::ptr_eq(
                &geometry,
                feed.frame()
                    .expect("frame")
                    .display_layout
                    .as_ref()
                    .expect("layout")
            ),
            "unchanged geometry must keep its Rc — a new one repaints the whole field"
        );
    }

    /// A refusal is permanent for the connection: the geometry asking
    /// stops (the engine would rebuild and re-measure only to refuse
    /// again) while the frames keep flowing without a layout.
    #[test]
    fn a_refused_layout_stops_the_geometry_asking_but_not_the_frames() {
        let mut feed = PreviewOutputFeed::default();
        let mut refused = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        refused.display_layout = ControlDisplayLayoutProbeResult::Unsupported {
            reason: "over the size budget".to_string(),
        };

        feed.apply(true, &[refused]);

        assert_eq!(feed.next_read(), Some(ControlDisplayLayoutRead::None));

        feed.apply(true, &[entry(4, 2, vec![9, 0, 8, 0, 7, 0])]);
        let frame = feed.frame().expect("frames keep flowing");
        assert!(frame.display_layout.is_none());
    }

    /// The small-dome regression (2026-08-29): a project driving TWO
    /// outputs previews BOTH. The feed used to latch the first entry that
    /// published, and the second box's 2,975 lamps never appeared anywhere.
    #[test]
    fn every_published_output_joins_the_composed_picture() {
        let mut feed = PreviewOutputFeed::default();
        let unpublished = OutputFrameEntry {
            channels: 0,
            bytes: Vec::new(),
            ..entry(2, 1, vec![1, 0, 2, 0, 3, 0])
        };
        let mut box_1 = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        box_1.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        let mut box_2 = entry(5, 1, vec![9, 0, 8, 0, 7, 0]);
        box_2.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(12));

        feed.apply(true, &[unpublished, box_1, box_2]);

        let frame = feed.frame().expect("composed frame");
        assert_eq!(
            frame.bytes.as_ref(),
            [1, 0, 2, 0, 3, 0, 9, 0, 8, 0, 7, 0],
            "both outputs' bytes ride the one picture; the unpublished one waits"
        );
        let ControlDisplayLayout::Layout2d(layout) =
            frame.display_layout.as_deref().expect("geometry");
        let starts: Vec<u32> = layout.lamps.iter().map(|lamp| lamp.sample_start).collect();
        assert_eq!(starts, vec![0, 3], "each output's lamps read its stretch");
    }

    /// A format the lamp renderer cannot read leaves the last good frame up
    /// rather than drawing garbage.
    #[test]
    fn an_unreadable_sample_format_is_ignored() {
        let mut feed = PreviewOutputFeed::default();
        feed.apply(true, &[entry(4, 1, vec![1, 0, 2, 0, 3, 0])]);

        let mut u8_frame = entry(4, 2, vec![1, 2, 3]);
        u8_frame.sample_format = WireChannelSampleFormat::U8;
        feed.apply(true, &[u8_frame]);

        assert_eq!(feed.frame().expect("frame").revision, 1);
        assert_eq!(feed.frame_revision(), 1);
    }

    /// Losing the runtime invalidates only the claims about it; the picture
    /// and the project-level verdict survive the re-lease.
    #[test]
    fn invalidating_the_runtime_keeps_the_frame_and_the_verdict() {
        let mut feed = PreviewOutputFeed::default();
        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        feed.apply(true, &[first]);

        feed.invalidate_runtime();

        assert_eq!(feed.control_first(), Some(true));
        assert_eq!(feed.frame().expect("last frame").revision, 1);
        assert_eq!(feed.next_read(), Some(ControlDisplayLayoutRead::Always));
    }

    fn entry(node: u32, revision: i64, bytes: Vec<u8>) -> OutputFrameEntry {
        OutputFrameEntry {
            node: NodeId::new(node),
            revision: Revision::new(revision),
            channels: (bytes.len() / 6) as u32,
            sample_format: WireChannelSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: vec![ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: (bytes.len() / 2) as u32,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: (bytes.len() / 6) as u32,
                        color_order: ColorOrder::Rgb,
                    },
                }],
            },
            display_layout: ControlDisplayLayoutProbeResult::Omitted,
            // These feeds render lamps, not the bay: one auto-flowed
            // producer over the whole wire is the shape they see.
            placements: Vec::new(),
            bytes,
        }
    }

    fn layout(revision: i64) -> ControlDisplayLayout {
        ControlDisplayLayout::Layout2d(layout_2d(revision))
    }

    fn layout_2d(revision: i64) -> ControlLayout2d {
        ControlLayout2d::new(
            Revision::new(revision),
            8,
            8,
            vec![ControlLamp2d {
                lamp_index: 0,
                sample_start: 0,
                center: [0.5, 0.5],
                radius: 0.05,
            }],
        )
    }
}

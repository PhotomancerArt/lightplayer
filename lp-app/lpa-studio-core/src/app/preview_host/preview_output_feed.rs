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
//!   keeps a STABLE one across frames whose geometry did not move — which is
//!   why the layout lives on the feed and each frame clones the one `Rc`.
//! - **One output node.** A project can drive several outputs; a card shows
//!   one. The first output that has actually published is latched and every
//!   later answer filters by it.
//!
//! The device card feed (`crate::app::runtime_pool::card_feed`) keeps the
//! same guarantees for a session at the far end of a serial link, where the
//! read is a host-driven pull with its own pacing and offline story. This one
//! rides a frame the preview host already schedules, so it owns neither.

use std::rc::Rc;

use lpc_model::{ControlDisplayLayout, ControlExtent, NodeId, Revision};
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, OutputFrameEntry,
    WireChannelSampleFormat,
};

use crate::{UiControlProductPreview, UiControlSampleFormat};

/// Output-frame state for one [`super::PreviewSlotHandle`].
#[derive(Debug, Default)]
pub struct PreviewOutputFeed {
    /// The engine's answer to "does the root scope resolve `control.out`?",
    /// `None` until the first frame answers.
    control_first: Option<bool>,
    /// The output node this slot follows, latched at the first frame.
    node: Option<NodeId>,
    /// The geometry every frame shares — one `Rc`, replaced only when the
    /// engine sends a different layout.
    layout: Option<Rc<ControlDisplayLayout>>,
    /// The engine-side revision of `layout`, for `IfChanged` gating.
    layout_revision: Option<Revision>,
    /// The engine refused the layout (dome-scale geometry is over the
    /// serialized-size budget). The feed stops asking — building and
    /// measuring a refused layout is real work at exactly the scale that can
    /// least afford it — and the consumer synthesizes one instead.
    layout_refused: bool,
    /// The newest frame, as the renderer consumes it.
    frame: Option<UiControlProductPreview>,
    /// Bumped once per genuinely new frame; the consumer's cheap poll.
    frame_revision: u64,
}

impl PreviewOutputFeed {
    /// The engine's control-first verdict, `None` until the first answer.
    pub fn control_first(&self) -> Option<bool> {
        self.control_first
    }

    /// The newest output frame, if one has landed.
    pub fn frame(&self) -> Option<&UiControlProductPreview> {
        self.frame.as_ref()
    }

    /// Bumped on every genuinely new frame — poll this to re-read cheaply.
    pub fn frame_revision(&self) -> u64 {
        self.frame_revision
    }

    /// Whether the feed still has no geometry to draw with (the engine
    /// refused it, or no frame has carried one yet).
    pub fn wants_display_layout(&self) -> bool {
        self.layout.is_none()
    }

    /// What the next preview frame should ask for: `None` once the engine
    /// has said this project is not control-first (no output traffic at all),
    /// otherwise the geometry gate — `Always` until a layout is known,
    /// `IfChanged` while one stands, `None` once the engine refused it.
    pub fn next_read(&self) -> Option<ControlDisplayLayoutRead> {
        if self.control_first == Some(false) {
            return None;
        }
        if self.layout_refused {
            return Some(ControlDisplayLayoutRead::None);
        }
        Some(match self.layout_revision {
            Some(revision) => ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(revision),
            },
            None => ControlDisplayLayoutRead::Always,
        })
    }

    /// Fold one `preview_output_frame` answer into the feed.
    pub fn apply(&mut self, control_first: bool, outputs: &[OutputFrameEntry]) {
        self.control_first = Some(control_first);
        let Some(entry) = self.pick_entry(outputs) else {
            return;
        };
        self.node = Some(entry.node);
        self.apply_display_layout(&entry.display_layout);

        let unchanged = self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.revision == entry.revision.0);
        // U16 is the only format the lamp renderer reads; anything else would
        // draw as garbage. Keep the last good frame instead.
        let readable = entry.sample_format == WireChannelSampleFormat::U16
            && entry.channels > 0
            && !entry.bytes.is_empty();
        if unchanged || !readable {
            return;
        }

        self.frame = Some(UiControlProductPreview {
            revision: entry.revision.0,
            // The published buffer is one row of RGB samples: three u16
            // channels per lamp.
            extent: ControlExtent::new(1, entry.channels.saturating_mul(3)),
            sample_format: UiControlSampleFormat::U16,
            sample_layout: entry.sample_layout.clone(),
            // The ONE geometry Rc, cloned — pointer-stable across frames.
            display_layout: self.layout.clone(),
            // A fresh Rc per frame: the renderer's repaint key.
            bytes: Rc::from(entry.bytes.as_slice()),
        });
        self.frame_revision += 1;
    }

    /// Install a consumer-synthesized display layout (the dome-scale
    /// fallback, for when the engine refused its own). It becomes the stable
    /// `Rc` later frames clone, and the frame already in hand adopts it so
    /// the slot draws without waiting for the next tick.
    pub fn set_synthesized_layout(&mut self, layout: Rc<ControlDisplayLayout>) {
        if let Some(frame) = self.frame.as_mut() {
            frame.display_layout = Some(Rc::clone(&layout));
        }
        self.layout = Some(layout);
        self.layout_revision = None;
    }

    /// Drop everything that was a claim about the slot's RUNTIME — the
    /// followed output and the geometry — when that runtime goes away
    /// (eviction, worker recycle). The last frame stays on screen and the
    /// control-first verdict stands: both are facts about the project, which
    /// a re-lease redeploys unchanged.
    pub fn invalidate_runtime(&mut self) {
        self.node = None;
        self.layout = None;
        self.layout_revision = None;
        self.layout_refused = false;
    }

    /// Pick this slot's entry out of one answer: the latched output node when
    /// there is one, else the first output that has actually published — an
    /// output with no frame yet is "no frame yet", never worth latching onto.
    fn pick_entry<'a>(&self, outputs: &'a [OutputFrameEntry]) -> Option<&'a OutputFrameEntry> {
        match self.node {
            Some(node) => outputs.iter().find(|entry| entry.node == node),
            None => outputs
                .iter()
                .find(|entry| entry.channels > 0 && !entry.bytes.is_empty()),
        }
    }

    /// Absorb the geometry half of an answer.
    fn apply_display_layout(&mut self, result: &ControlDisplayLayoutProbeResult) {
        match result {
            ControlDisplayLayoutProbeResult::Layout(layout) => {
                self.layout_revision = Some(layout.revision());
                self.layout_refused = false;
                self.layout = Some(Rc::new(layout.clone()));
            }
            // Both mean "what you have still stands" — keep the Rc, which is
            // the whole point of asking `IfChanged`.
            ControlDisplayLayoutProbeResult::Unchanged { .. }
            | ControlDisplayLayoutProbeResult::Omitted => {}
            ControlDisplayLayoutProbeResult::Unsupported { .. } => {
                self.layout_refused = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ColorOrder, ControlLamp2d, ControlLayout2d, ControlSampleEncoding, ControlSampleLayout,
        ControlSampleSpan,
    };

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
        assert_eq!(frame.extent, ControlExtent::new(1, 3));
        assert_eq!(frame.sample_format, UiControlSampleFormat::U16);
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

    #[test]
    fn a_refused_layout_stops_the_geometry_asking_but_not_the_frames() {
        let mut feed = PreviewOutputFeed::default();
        let mut refused = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        refused.display_layout = ControlDisplayLayoutProbeResult::Unsupported {
            reason: "over the size budget".to_string(),
        };

        feed.apply(true, &[refused]);

        assert!(feed.wants_display_layout());
        assert_eq!(feed.next_read(), Some(ControlDisplayLayoutRead::None));

        let synthesized = Rc::new(ControlDisplayLayout::Layout2d(layout_2d(11)));
        feed.set_synthesized_layout(Rc::clone(&synthesized));

        feed.apply(true, &[entry(4, 2, vec![9, 0, 8, 0, 7, 0])]);
        assert!(Rc::ptr_eq(
            &synthesized,
            feed.frame()
                .expect("frame")
                .display_layout
                .as_ref()
                .expect("layout")
        ));
        assert!(!feed.wants_display_layout());
    }

    #[test]
    fn the_first_published_output_is_latched_and_keeps_the_slot_on_one_node() {
        let mut feed = PreviewOutputFeed::default();
        let unpublished = OutputFrameEntry {
            channels: 0,
            bytes: Vec::new(),
            ..entry(2, 1, vec![1, 0, 2, 0, 3, 0])
        };

        feed.apply(true, &[unpublished, entry(5, 1, vec![1, 0, 2, 0, 3, 0])]);
        assert_eq!(feed.node, Some(NodeId::new(5)));

        // A second output appearing later cannot swap the picture.
        feed.apply(
            true,
            &[
                entry(2, 4, vec![7, 0, 7, 0, 7, 0]),
                entry(5, 4, vec![1, 0, 2, 0, 3, 0]),
            ],
        );

        assert_eq!(feed.node, Some(NodeId::new(5)));
        assert_eq!(
            feed.frame().expect("frame").bytes.as_ref(),
            [1, 0, 2, 0, 3, 0]
        );
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

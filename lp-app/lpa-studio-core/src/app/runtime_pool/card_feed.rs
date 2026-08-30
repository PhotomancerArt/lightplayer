//! One device session's live frame feed — the state behind a card's ▶ tab.
//!
//! The feed is the client half of the published-frame read (P1's
//! [`OutputFrameProbeRequest`]): while a card's ▶ Play tab is selected on a
//! Ready device, the studio pulls the frame the board has ALREADY published
//! at [`DEVICE_CARD_FEED_INTERVAL`](crate::DEVICE_CARD_FEED_INTERVAL)
//! completion-gap and keeps the newest one here.
//!
//! # What this state exists to guarantee
//!
//! - **Revision-only change detection.** A new frame is a frame whose
//!   `revision` moved — the buffer's `changed_at`, which advances only when
//!   the device publishes. Arrival time never counts: a pull that answers
//!   with the same revisions (the device is idle, or paused) leaves the
//!   frame, its bytes `Rc`, and its age stamp exactly as they were, so the
//!   card ages honestly toward the stale threshold instead of pretending a
//!   re-read is a new picture.
//! - **The LampView `Rc` contract.** The renderer repaints on `Rc` POINTER
//!   identity, so every genuinely new frame gets a FRESH `bytes` `Rc` and
//!   the display layout keeps a STABLE one across frames whose geometry did
//!   not move. The per-output folding and the composed picture both live in
//!   [`OutputFrameCache`], which keeps those identities.
//! - **Every output joins the picture.** A project can drive several
//!   outputs, and the card composes ALL of them
//!   ([`OutputFrameCache::composed_frame`]) — one buffer, every wire's
//!   lamps at their own offsets. The feed used to latch the FIRST output
//!   that published, which is how the small dome's second box (2,975
//!   lamps) never appeared on the sim card.
//! - **Last-known survives the session going dark.** Nothing here is
//!   cleared on disconnect (Q4: offline shows the last in-session frame,
//!   dimmed). Only the wire-scoped facts — the project handle and the
//!   per-output geometry claims — are invalidated, because those are claims
//!   about a connection.
//!
//! [`OutputFrameProbeRequest`]: lpc_wire::OutputFrameProbeRequest

use lpc_wire::{ControlDisplayLayoutRead, OutputFrameEntry};

use crate::UiControlProductPreview;
use crate::app::project::output_frame_cache::OutputFrameCache;

/// What applying a pulled answer did, for the caller's follow-up work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CardFeedApply {
    /// A revision moved: a genuinely new frame landed.
    pub new_frame: bool,
}

/// The live frame feed for one device session.
#[derive(Debug, Default)]
pub struct CardFeedState {
    /// The device's loaded-project handle, acquired once per connection.
    handle_id: Option<u32>,
    /// The per-output half: every output's newest frame and geometry, plus
    /// the read gate — all connection-scoped claims, reset on invalidate.
    outputs: OutputFrameCache,
    /// The newest COMPOSED picture, as the renderer consumes it. Held
    /// outside [`Self::outputs`] so it survives `invalidate_connection`
    /// (the offline card's last picture) while the claims behind it reset.
    frame: Option<UiControlProductPreview>,
    /// When the newest frame's revision was first observed (injected-clock
    /// epoch seconds) — the age the card's meta row reads.
    last_frame_at: Option<f64>,
    /// When the last feed pull COMPLETED, for completion-based pacing.
    last_pull_completed_at: Option<f64>,
    /// The card identity this feed has been feeding, recorded at pull time.
    ///
    /// It exists for the moment the link dies: the roster then drops the
    /// live row in favour of the better-informed REGISTRY card, and the
    /// registry card is keyed by uid — while the session, whose reconcile
    /// bundle a failed pull just cleared, can no longer name that uid. The
    /// key it was feeding a second ago is the durable answer, and it is
    /// exactly the identity the offline card wears.
    card_key: Option<String>,
}

impl CardFeedState {
    /// The device's loaded-project handle, once acquired.
    pub fn handle_id(&self) -> Option<u32> {
        self.handle_id
    }

    pub fn set_handle_id(&mut self, handle_id: u32) {
        self.handle_id = Some(handle_id);
    }

    /// Drop everything that was a claim about the CONNECTION: the handle
    /// and the per-output state (which a reload may have changed under us —
    /// nodes, geometry, refusals alike). The last composed frame and its
    /// age stay — an offline card shows the last thing the board actually
    /// did.
    pub fn invalidate_connection(&mut self) {
        self.handle_id = None;
        self.outputs = OutputFrameCache::default();
    }

    /// The newest composed frame, if this session has ever carried one.
    pub fn frame(&self) -> Option<&UiControlProductPreview> {
        self.frame.as_ref()
    }

    /// When the newest frame arrived (injected-clock epoch seconds).
    pub fn last_frame_at(&self) -> Option<f64> {
        self.last_frame_at
    }

    /// How old the newest frame is at `now`, in seconds. Never negative —
    /// a clock that stepped backwards reads as "just now" rather than as a
    /// frame from the future.
    pub fn frame_age_secs(&self, now: f64) -> Option<f64> {
        self.last_frame_at.map(|at| (now - at).max(0.0))
    }

    /// The card identity this feed feeds (see the field's docs).
    pub fn card_key(&self) -> Option<&str> {
        self.card_key.as_deref()
    }

    /// Record the card identity this feed is feeding. A card's key can move
    /// once — an anonymous board's session key becomes its uid the moment
    /// identity resolves — and the newest answer is the right one.
    pub fn set_card_key(&mut self, card_key: impl Into<String>) {
        self.card_key = Some(card_key.into());
    }

    /// When the last feed pull completed (injected-clock epoch seconds).
    /// The pacing stamp — read by tests and diagnostics; the cadence
    /// decision itself is [`Self::due`].
    pub fn pull_completed_at(&self) -> Option<f64> {
        self.last_pull_completed_at
    }

    /// What the next pull should ask for — the shared multi-output gate
    /// ([`OutputFrameCache::display_layout_read`]): `Always` while any
    /// output still lacks geometry, `IfChanged` while every layout stands,
    /// and `None` once every output that could answer has refused — a
    /// refusal costs the engine a full layout build plus a
    /// serialized-length measurement, so re-asking every 150 ms would tax
    /// exactly the dome-scale board that can least afford it.
    pub fn display_layout_read(&self) -> ControlDisplayLayoutRead {
        self.outputs.display_layout_read()
    }

    /// Fold one pulled answer — every output entry it carried — into the
    /// feed, and recompose the picture.
    ///
    /// `now` only ever stamps an answer that carried a MOVED revision.
    /// Everything else — repeated revisions, unreadable sample formats —
    /// leaves the stored frame and its age untouched.
    pub fn apply(&mut self, entries: &[OutputFrameEntry], now: f64) -> CardFeedApply {
        let before = self.outputs.frames_seen();
        self.outputs.apply(entries);
        let new_frame = self.outputs.frames_seen() != before;
        let nodes: Vec<lpc_model::NodeId> = self.outputs.outputs().collect();
        if let Some(composed) = self.outputs.composed_frame(&nodes) {
            self.frame = Some(composed);
        }
        if new_frame {
            self.last_frame_at = Some(now);
        }
        CardFeedApply { new_frame }
    }

    /// Whether a feed pull is due at `now` under the completion `gap`. A
    /// feed that never pulled is due at once.
    pub fn due(&self, now: f64, gap: core::time::Duration) -> bool {
        self.due_in(now, gap) == core::time::Duration::ZERO
    }

    /// Time until the next feed pull is due, for the actor's
    /// min-over-sessions delay.
    pub fn due_in(&self, now: f64, gap: core::time::Duration) -> core::time::Duration {
        match self.last_pull_completed_at {
            None => core::time::Duration::ZERO,
            Some(last) => {
                let elapsed = (now - last).max(0.0);
                gap.saturating_sub(core::time::Duration::from_secs_f64(elapsed))
            }
        }
    }

    /// Stamp a feed pull's completion: the next one is due one gap later,
    /// counted from when this one FINISHED, so a slow dome frame spaces
    /// itself out instead of running back-to-back.
    pub fn mark_pull_complete(&mut self, now: f64) {
        self.last_pull_completed_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use lpc_model::{
        ColorOrder, ControlDisplayLayout, ControlLamp2d, ControlLayout2d, ControlSampleEncoding,
        ControlSampleLayout, ControlSampleSpan, NodeId, Revision,
    };
    use lpc_wire::{ControlDisplayLayoutProbeResult, WireChannelSampleFormat};

    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn a_moved_revision_is_a_new_frame_with_fresh_bytes() {
        let mut feed = CardFeedState::default();

        feed.apply(&[entry(4, 1, vec![1, 0, 2, 0, 3, 0])], NOW);
        let first = Rc::clone(&feed.frame().expect("first frame").bytes);
        let applied = feed.apply(&[entry(4, 2, vec![9, 0, 8, 0, 7, 0])], NOW + 0.2);

        assert!(applied.new_frame);
        assert_eq!(feed.frame().expect("second frame").revision, 2);
        assert!(
            !Rc::ptr_eq(&first, &feed.frame().expect("second frame").bytes),
            "every new frame must carry a fresh bytes Rc — the renderer's repaint key"
        );
        assert_eq!(feed.last_frame_at(), Some(NOW + 0.2));
    }

    #[test]
    fn a_repeated_revision_is_not_a_new_frame_and_does_not_refresh_the_age() {
        let mut feed = CardFeedState::default();
        feed.apply(&[entry(4, 7, vec![1, 0, 2, 0, 3, 0])], NOW);
        let bytes = Rc::clone(&feed.frame().expect("frame").bytes);

        // Same revision, different bytes: the device did not publish, so
        // this is the same picture however late it arrived.
        let applied = feed.apply(&[entry(4, 7, vec![4, 0, 5, 0, 6, 0])], NOW + 3.0);

        assert!(!applied.new_frame);
        assert!(Rc::ptr_eq(&bytes, &feed.frame().expect("frame").bytes));
        assert_eq!(feed.last_frame_at(), Some(NOW));
        assert_eq!(feed.frame_age_secs(NOW + 3.0), Some(3.0));
    }

    #[test]
    fn the_layout_rc_stays_stable_while_the_device_says_unchanged() {
        let mut feed = CardFeedState::default();
        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));

        feed.apply(&[first], NOW);
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
        feed.apply(&[second], NOW + 0.2);

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
    fn the_first_pull_asks_always_and_later_ones_ask_if_changed() {
        let mut feed = CardFeedState::default();
        assert_eq!(feed.display_layout_read(), ControlDisplayLayoutRead::Always);

        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        feed.apply(&[first], NOW);

        assert_eq!(
            feed.display_layout_read(),
            ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(Revision::new(11)),
            }
        );
    }

    /// A refusal is permanent for the connection: the feed stops asking so
    /// the engine stops re-building and re-measuring a layout it will
    /// refuse again — and the card stays honestly layout-less.
    #[test]
    fn a_refused_layout_stops_the_asking() {
        let mut feed = CardFeedState::default();
        let mut refused = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        refused.display_layout = ControlDisplayLayoutProbeResult::Unsupported {
            reason: "over the wire budget".to_string(),
        };

        feed.apply(&[refused], NOW);

        assert_eq!(feed.display_layout_read(), ControlDisplayLayoutRead::None);
        assert!(feed.frame().expect("frame").display_layout.is_none());
    }

    /// The small-dome regression (2026-08-29): a project driving TWO
    /// outputs shows BOTH on the card. The feed used to latch the first
    /// entry that published, and the second box's 2,975 lamps never
    /// appeared anywhere.
    #[test]
    fn every_published_output_joins_the_composed_picture() {
        let mut feed = CardFeedState::default();
        let mut box_1 = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        box_1.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        let mut box_2 = entry(5, 1, vec![9, 0, 8, 0, 7, 0]);
        box_2.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(12));

        feed.apply(&[box_1, box_2], NOW);

        let frame = feed.frame().expect("composed frame");
        assert_eq!(
            frame.bytes.as_ref(),
            [1, 0, 2, 0, 3, 0, 9, 0, 8, 0, 7, 0],
            "both outputs' bytes ride the one picture"
        );
        let ControlDisplayLayout::Layout2d(layout) =
            frame.display_layout.as_deref().expect("geometry");
        let starts: Vec<u32> = layout.lamps.iter().map(|lamp| lamp.sample_start).collect();
        assert_eq!(starts, vec![0, 3], "each output's lamps read its stretch");
    }

    /// An output with no published frame contributes nothing yet — the
    /// composed picture is the outputs that HAVE published.
    #[test]
    fn an_unpublished_output_is_not_part_of_the_picture() {
        let mut feed = CardFeedState::default();
        let empty = OutputFrameEntry {
            channels: 0,
            bytes: Vec::new(),
            ..entry(2, 1, vec![1, 0, 2, 0, 3, 0])
        };
        let live = entry(5, 1, vec![1, 0, 2, 0, 3, 0]);

        feed.apply(&[empty, live], NOW);

        assert_eq!(
            feed.frame().expect("frame").bytes.as_ref(),
            [1, 0, 2, 0, 3, 0]
        );
    }

    #[test]
    fn invalidating_the_connection_keeps_the_last_frame() {
        let mut feed = CardFeedState::default();
        feed.set_handle_id(3);
        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        feed.apply(&[first], NOW);

        feed.invalidate_connection();

        assert_eq!(feed.handle_id(), None);
        assert_eq!(feed.display_layout_read(), ControlDisplayLayoutRead::Always);
        assert_eq!(feed.frame().expect("last frame").revision, 1);
        assert_eq!(feed.frame_age_secs(NOW + 9.0), Some(9.0));
    }

    #[test]
    fn pulls_pace_from_completion() {
        let gap = core::time::Duration::from_millis(150);
        let mut feed = CardFeedState::default();

        assert!(
            feed.due(NOW, gap),
            "a feed that never pulled is due at once"
        );
        feed.mark_pull_complete(NOW);
        assert!(!feed.due(NOW + 0.1, gap));
        assert!(feed.due(NOW + 0.15, gap));
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

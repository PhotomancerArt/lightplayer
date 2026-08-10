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
//!   with the same revision (the device is idle, or paused) leaves the
//!   frame, its bytes `Rc`, and its age stamp exactly as they were, so the
//!   card ages honestly toward the stale threshold instead of pretending a
//!   re-read is a new picture.
//! - **The LampView `Rc` contract.** The renderer repaints on `Rc` POINTER
//!   identity, so every genuinely new frame gets a FRESH `bytes` `Rc` and
//!   the display layout keeps a STABLE one across frames whose geometry did
//!   not move. That is why the layout lives on the feed rather than inside
//!   each preview: the per-frame preview clones the one `Rc` the feed holds.
//! - **One output node.** A project can drive several outputs; the card
//!   shows one. The first entry that has actually published is latched
//!   ([`CardFeedState::node`]) and every later pull filters by it, so a
//!   second output appearing mid-session cannot swap the picture.
//! - **Last-known survives the session going dark.** Nothing here is
//!   cleared on disconnect (Q4: offline shows the last in-session frame,
//!   dimmed). Only the wire-scoped facts — the project handle and the
//!   geometry — are invalidated, because those are claims about a
//!   connection.

use std::rc::Rc;

use lpc_model::{ControlDisplayLayout, ControlExtent, NodeId, Revision};
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, OutputFrameEntry,
    WireChannelSampleFormat,
};

use crate::{UiControlProductPreview, UiControlSampleFormat};

/// What applying a pulled entry did, for the caller's follow-up work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CardFeedApply {
    /// The revision moved: a genuinely new frame landed.
    pub new_frame: bool,
    /// The device refused (or omitted) the display layout and the feed has
    /// none, so the caller should synthesize one locally. Dome-scale
    /// fixtures live here permanently — the layout is over the wire budget.
    pub wants_layout: bool,
}

/// The live frame feed for one device session.
#[derive(Debug, Default)]
pub struct CardFeedState {
    /// The device's loaded-project handle, acquired once per connection.
    handle_id: Option<u32>,
    /// The output node this card follows, latched at the first frame.
    node: Option<NodeId>,
    /// The geometry every preview shares — one `Rc`, replaced only when the
    /// device sends a different layout or a local synthesis lands.
    layout: Option<Rc<ControlDisplayLayout>>,
    /// The engine-side revision of `layout`, for `IfChanged` gating. `None`
    /// while the layout is client-synthesized (the refusal never tells us
    /// the engine's layout revision).
    layout_revision: Option<Revision>,
    /// The device answered `Unsupported` for this connection's geometry.
    /// The feed stops asking (the engine builds and measures the whole
    /// layout to refuse it — real work at dome scale, every pull) and the
    /// client-side synthesis stands in until the handle is invalidated.
    layout_refused: bool,
    /// The newest frame, as the renderer consumes it.
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

    /// Drop everything that was a claim about the CONNECTION: the handle,
    /// the followed output, and the geometry (which a reload may have
    /// changed under us). The last frame and its age stay — an offline card
    /// shows the last thing the board actually did.
    pub fn invalidate_connection(&mut self) {
        self.handle_id = None;
        self.node = None;
        self.layout = None;
        self.layout_revision = None;
        self.layout_refused = false;
    }

    /// The newest frame, if this session has ever carried one.
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

    /// The output node this card follows, once latched.
    pub fn node(&self) -> Option<NodeId> {
        self.node
    }

    /// Whether the feed still has no geometry to draw with.
    pub fn wants_layout(&self) -> bool {
        self.layout.is_none()
    }

    /// What the next pull should ask for.
    ///
    /// `Always` until geometry is known, `IfChanged` while the device's own
    /// layout stands, and `None` once the device refused it — a refusal
    /// costs the engine a full layout build plus a serialized-length
    /// measurement, so re-asking every 150 ms would tax exactly the
    /// dome-scale board that can least afford it.
    pub fn display_layout_read(&self) -> ControlDisplayLayoutRead {
        if self.layout_refused {
            return ControlDisplayLayoutRead::None;
        }
        match self.layout_revision {
            Some(revision) => ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(revision),
            },
            None => ControlDisplayLayoutRead::Always,
        }
    }

    /// Install a client-synthesized display layout (the dome-scale fallback).
    /// It becomes the stable `Rc` every later preview clones, and the
    /// current frame — already built — adopts it too, so the card draws on
    /// the same pull the synthesis ran on.
    pub fn set_synthesized_layout(&mut self, layout: Rc<ControlDisplayLayout>) {
        if let Some(frame) = self.frame.as_mut() {
            frame.display_layout = Some(Rc::clone(&layout));
        }
        self.layout = Some(layout);
        self.layout_revision = None;
    }

    /// Pick this card's entry out of a pulled result: the latched output
    /// node when there is one, else the first output that has actually
    /// published (`channels > 0`) — an output with no frame yet is "no
    /// frame yet", never a fault, and never worth latching onto.
    pub fn pick_entry<'a>(&self, outputs: &'a [OutputFrameEntry]) -> Option<&'a OutputFrameEntry> {
        match self.node {
            Some(node) => outputs.iter().find(|entry| entry.node == node),
            None => outputs
                .iter()
                .find(|entry| entry.channels > 0 && !entry.bytes.is_empty()),
        }
    }

    /// Fold one pulled entry into the feed.
    ///
    /// `now` only ever stamps a frame whose revision MOVED. Everything else
    /// — a repeated revision, an unreadable sample format — leaves the
    /// stored frame and its age untouched.
    pub fn apply_entry(&mut self, entry: &OutputFrameEntry, now: f64) -> CardFeedApply {
        self.node = Some(entry.node);
        self.apply_display_layout(&entry.display_layout);

        let unchanged = self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.revision == entry.revision.0);
        // U16 is the only format Studio can read today; a U8 device would
        // otherwise be shown as garbage. Keep the last good frame instead.
        let readable = entry.sample_format == WireChannelSampleFormat::U16
            && entry.channels > 0
            && !entry.bytes.is_empty();
        if unchanged || !readable {
            return CardFeedApply {
                new_frame: false,
                wants_layout: self.wants_layout(),
            };
        }

        self.frame = Some(UiControlProductPreview {
            revision: entry.revision.0,
            // The published buffer is one row of RGB samples: three u16
            // channels per lamp, which is what `bytes.len()` reports.
            extent: ControlExtent::new(1, entry.channels.saturating_mul(3)),
            sample_format: UiControlSampleFormat::U16,
            sample_layout: entry.sample_layout.clone(),
            // The ONE geometry Rc, cloned — pointer-stable across frames.
            display_layout: self.layout.clone(),
            // A fresh Rc per frame: the renderer's repaint key.
            bytes: Rc::from(entry.bytes.as_slice()),
        });
        self.last_frame_at = Some(now);
        CardFeedApply {
            new_frame: true,
            wants_layout: self.wants_layout(),
        }
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

    /// Absorb the geometry half of a pulled entry.
    fn apply_display_layout(&mut self, result: &ControlDisplayLayoutProbeResult) {
        match result {
            ControlDisplayLayoutProbeResult::Layout(layout) => {
                self.layout_revision = Some(layout.revision());
                self.layout_refused = false;
                self.layout = Some(Rc::new(layout.clone()));
            }
            // Both mean "what you have still stands" — keep the Rc, which
            // is the whole point of asking `IfChanged`.
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

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn a_moved_revision_is_a_new_frame_with_fresh_bytes() {
        let mut feed = CardFeedState::default();

        feed.apply_entry(&entry(4, 1, vec![1, 0, 2, 0, 3, 0]), NOW);
        let first = Rc::clone(&feed.frame().expect("first frame").bytes);
        let applied = feed.apply_entry(&entry(4, 2, vec![9, 0, 8, 0, 7, 0]), NOW + 0.2);

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
        feed.apply_entry(&entry(4, 7, vec![1, 0, 2, 0, 3, 0]), NOW);
        let bytes = Rc::clone(&feed.frame().expect("frame").bytes);

        // Same revision, different bytes: the device did not publish, so
        // this is the same picture however late it arrived.
        let applied = feed.apply_entry(&entry(4, 7, vec![4, 0, 5, 0, 6, 0]), NOW + 3.0);

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

        feed.apply_entry(&first, NOW);
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
        feed.apply_entry(&second, NOW + 0.2);

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
        feed.apply_entry(&first, NOW);

        assert_eq!(
            feed.display_layout_read(),
            ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(Revision::new(11)),
            }
        );
    }

    #[test]
    fn a_refused_layout_stops_the_asking_and_takes_a_synthesis() {
        let mut feed = CardFeedState::default();
        let mut refused = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        refused.display_layout = ControlDisplayLayoutProbeResult::Unsupported {
            reason: "over the wire budget".to_string(),
        };

        let applied = feed.apply_entry(&refused, NOW);

        assert!(applied.wants_layout);
        assert_eq!(feed.display_layout_read(), ControlDisplayLayoutRead::None);

        let synthesized = Rc::new(ControlDisplayLayout::Layout2d(layout_2d(11)));
        feed.set_synthesized_layout(Rc::clone(&synthesized));

        // The frame already in hand adopts it, and the next one shares it.
        assert!(Rc::ptr_eq(
            &synthesized,
            feed.frame()
                .expect("frame")
                .display_layout
                .as_ref()
                .expect("layout")
        ));
        feed.apply_entry(&entry(4, 2, vec![9, 0, 8, 0, 7, 0]), NOW + 0.2);
        assert!(Rc::ptr_eq(
            &synthesized,
            feed.frame()
                .expect("frame")
                .display_layout
                .as_ref()
                .expect("layout")
        ));
        assert!(!feed.wants_layout());
    }

    #[test]
    fn an_output_with_no_published_frame_is_not_latched() {
        let feed = CardFeedState::default();
        let empty = OutputFrameEntry {
            channels: 0,
            bytes: Vec::new(),
            ..entry(2, 1, vec![1, 0, 2, 0, 3, 0])
        };
        let live = entry(5, 1, vec![1, 0, 2, 0, 3, 0]);

        let outputs = [empty, live];
        let picked = feed.pick_entry(&outputs).expect("a published output");

        assert_eq!(picked.node, NodeId::new(5));
    }

    #[test]
    fn the_latched_output_keeps_the_card_on_one_node() {
        let mut feed = CardFeedState::default();
        feed.apply_entry(&entry(5, 1, vec![1, 0, 2, 0, 3, 0]), NOW);

        let outputs = vec![
            entry(2, 4, vec![7, 0, 7, 0, 7, 0]),
            entry(5, 4, vec![1, 0, 2, 0, 3, 0]),
        ];

        assert_eq!(
            feed.pick_entry(&outputs).expect("latched output").node,
            NodeId::new(5)
        );
    }

    #[test]
    fn invalidating_the_connection_keeps_the_last_frame() {
        let mut feed = CardFeedState::default();
        feed.set_handle_id(3);
        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        feed.apply_entry(&first, NOW);

        feed.invalidate_connection();

        assert_eq!(feed.handle_id(), None);
        assert_eq!(feed.node(), None);
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

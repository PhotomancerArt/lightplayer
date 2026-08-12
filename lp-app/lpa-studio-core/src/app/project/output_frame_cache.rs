//! The LENS's view of the frames the outputs have already published.
//!
//! A module that owns an Output node heroes the wire that output composes —
//! every producer's lamps, patched into one strand — and nothing else in the
//! lens can show it. The output node publishes no product ([`ProjectSync`]'s
//! product previews are keyed by `UiProductRef`, and there is no product ref
//! for a sink), so the composed picture is reachable only through the
//! published-frame probe, keyed by output NODE. This is where those answers
//! live between reads.
//!
//! # What this state exists to guarantee
//!
//! - **The geometry is the OUTPUT's, not a producer's.** The merged display
//!   layout arrives on the frame (the engine's
//!   `merge_fragment_display_layouts`) with
//!   every lamp's `sample_start` rebased onto the wire, so a consumer indexes
//!   the frame's bytes by it and gets that lamp's own colour. Nothing here
//!   synthesizes geometry: a frame without a layout draws no lamps, which is
//!   the honest answer.
//! - **The `LampView` `Rc` contract.** The renderer repaints on `Rc` POINTER
//!   identity, so a genuinely new frame gets a FRESH `bytes` `Rc` while the
//!   layout keeps a STABLE one across frames whose geometry did not move —
//!   which is why the layout is cached per output rather than rebuilt into
//!   each preview.
//! - **Revision-only change detection.** A repeated revision leaves the
//!   cached preview, and its `Rc`s, exactly as they were.
//!
//! The device card's feed (`crate::app::runtime_pool::card_feed`) and the
//! preview host's (`crate::app::preview_host::preview_output_feed`) keep the
//! same guarantees for their own transports; this one owns no pacing, no
//! connection, and no fallback, because the lens read already has all three.
//!
//! [`ProjectSync`]: super::project_sync::ProjectSync

use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_model::{ControlDisplayLayout, ControlExtent, NodeId, Revision};
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, OutputFrameEntry,
    WireChannelSampleFormat, WireOutputPlacement,
};

use crate::{UiControlProductPreview, UiControlSampleFormat};

/// One output node's cached published frame.
#[derive(Debug, Default)]
struct OutputFrameEntryState {
    /// The geometry every preview of this output shares — one `Rc`, replaced
    /// only when the engine sends a different layout.
    layout: Option<Rc<ControlDisplayLayout>>,
    /// The engine-side revision of `layout`, for `IfChanged` gating.
    layout_revision: Option<Revision>,
    /// The engine refused this output's geometry (dome-scale layouts are over
    /// the serialized-size budget). The lens stops asking: building and
    /// measuring a refused layout is real work at exactly the scale that can
    /// least afford it, and the lens has no synthesis to install in its place.
    layout_refused: bool,
    /// The newest frame, as the renderer consumes it.
    frame: Option<UiControlProductPreview>,
    /// How the newest frame was CUT: one run per producer placed on this
    /// wire. Kept beside the frame rather than folded into it because it is
    /// not something the lamp renderer reads — it is what the patch bay
    /// draws, and the only description of which fixture owns which stretch
    /// of the strand (D34a).
    ///
    /// Replaced on every probe answer, including one whose revision
    /// repeated: a patch edit re-cuts the wire without republishing bytes,
    /// which is the same reason the layout is refreshed below.
    placements: Vec<WireOutputPlacement>,
}

/// Published output frames for the lens, keyed by output node.
#[derive(Debug, Default)]
pub struct OutputFrameCache {
    outputs: BTreeMap<NodeId, OutputFrameEntryState>,
}

impl OutputFrameCache {
    /// The newest frame one output published, if a read has carried one.
    pub fn frame(&self, node: NodeId) -> Option<&UiControlProductPreview> {
        self.outputs.get(&node)?.frame.as_ref()
    }

    /// How that output's wire is cut — its runs, in planning order. Empty
    /// for an output that has not answered, or has not planned yet.
    pub fn placements(&self, node: NodeId) -> &[WireOutputPlacement] {
        self.outputs
            .get(&node)
            .map_or(&[], |output| output.placements.as_slice())
    }

    /// Every output that has answered a probe, in node order — the set the
    /// patch bay walks (a fixture's cells may be on any of them).
    pub fn outputs(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.outputs.keys().copied()
    }

    /// What the next read should ask for.
    ///
    /// One gate covers every output on the probe (the request has no node
    /// selector), so the strictest answer wins: `Always` while ANY output
    /// still lacks geometry, `None` once every output that could answer has
    /// refused, and `IfChanged` otherwise. The `IfChanged` revision is the
    /// MINIMUM across outputs, so an output whose geometry moved still gets
    /// its new layout while its neighbours answer `Unchanged`.
    pub fn display_layout_read(&self) -> ControlDisplayLayoutRead {
        let mut known: Option<Revision> = None;
        let mut all_refused = true;
        for output in self.outputs.values() {
            if output.layout_refused {
                continue;
            }
            all_refused = false;
            match output.layout_revision {
                Some(revision) => {
                    known = Some(known.map_or(revision, |lowest: Revision| lowest.min(revision)));
                }
                // This output has never answered with geometry: ask outright.
                None => return ControlDisplayLayoutRead::Always,
            }
        }
        if self.outputs.is_empty() {
            return ControlDisplayLayoutRead::Always;
        }
        if all_refused {
            return ControlDisplayLayoutRead::None;
        }
        ControlDisplayLayoutRead::IfChanged {
            known_revision: known,
        }
    }

    /// Fold one probe answer — every output entry it carried — into the cache.
    pub fn apply(&mut self, outputs: &[OutputFrameEntry]) {
        for entry in outputs {
            self.apply_entry(entry);
        }
    }

    fn apply_entry(&mut self, entry: &OutputFrameEntry) {
        let output = self.outputs.entry(entry.node).or_default();
        // The cut, always: it is small, ungated, and moves under a repeated
        // frame revision whenever a patch is edited.
        if output.placements != entry.placements {
            output.placements = entry.placements.clone();
        }
        match &entry.display_layout {
            ControlDisplayLayoutProbeResult::Layout(layout) => {
                output.layout_revision = Some(layout.revision());
                output.layout_refused = false;
                output.layout = Some(Rc::new(layout.clone()));
            }
            // Both mean "what you have still stands" — keep the `Rc`, which is
            // the whole point of asking `IfChanged`.
            ControlDisplayLayoutProbeResult::Unchanged { .. }
            | ControlDisplayLayoutProbeResult::Omitted => {}
            ControlDisplayLayoutProbeResult::Unsupported { .. } => {
                output.layout_refused = true;
            }
        }

        let unchanged = output
            .frame
            .as_ref()
            .is_some_and(|frame| frame.revision == entry.revision.0);
        // U16 is the only format the lamp renderer reads; anything else would
        // draw as garbage. Keep the last good frame instead.
        let readable = entry.sample_format == WireChannelSampleFormat::U16
            && entry.channels > 0
            && !entry.bytes.is_empty();
        if unchanged {
            // Geometry may still have moved under a frame that did not: a
            // patch edit re-places the wire without republishing bytes.
            if let Some(frame) = output.frame.as_mut() {
                frame.display_layout = output.layout.clone();
            }
            return;
        }
        if !readable {
            return;
        }
        output.frame = Some(UiControlProductPreview {
            revision: entry.revision.0,
            // The published buffer is one row of RGB samples: three u16
            // channels per lamp.
            extent: ControlExtent::new(1, entry.channels.saturating_mul(3)),
            sample_format: UiControlSampleFormat::U16,
            sample_layout: entry.sample_layout.clone(),
            // The ONE geometry Rc, cloned — pointer-stable across frames.
            display_layout: output.layout.clone(),
            // A fresh Rc per frame: the renderer's repaint key.
            bytes: Rc::from(entry.bytes.as_slice()),
        });
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ColorOrder, ControlLamp2d, ControlLayout2d, ControlSampleEncoding, ControlSampleLayout,
        ControlSampleSpan,
    };

    use super::*;

    #[test]
    fn the_first_read_asks_always_and_later_ones_ask_if_changed() {
        let mut cache = OutputFrameCache::default();
        assert_eq!(
            cache.display_layout_read(),
            ControlDisplayLayoutRead::Always
        );

        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        cache.apply(&[first]);

        assert_eq!(
            cache.display_layout_read(),
            ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(Revision::new(11)),
            }
        );
    }

    /// The layout `Rc` is the renderer's repaint key for the whole lamp
    /// field: an `Unchanged` answer must hand back the SAME one.
    #[test]
    fn unchanged_geometry_keeps_its_rc_while_new_bytes_arrive() {
        let mut cache = OutputFrameCache::default();
        let mut first = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        cache.apply(&[first]);
        let geometry = Rc::clone(
            cache
                .frame(NodeId::new(4))
                .expect("frame")
                .display_layout
                .as_ref()
                .expect("layout"),
        );
        let bytes = Rc::clone(&cache.frame(NodeId::new(4)).expect("frame").bytes);

        let mut second = entry(4, 2, vec![9, 0, 8, 0, 7, 0]);
        second.display_layout = ControlDisplayLayoutProbeResult::Unchanged {
            revision: Revision::new(11),
        };
        cache.apply(&[second]);

        let frame = cache.frame(NodeId::new(4)).expect("frame");
        assert!(Rc::ptr_eq(
            &geometry,
            frame.display_layout.as_ref().expect("layout")
        ));
        assert!(
            !Rc::ptr_eq(&bytes, &frame.bytes),
            "a new frame must carry a fresh bytes Rc"
        );
    }

    /// A patch edit re-places the wire without moving the published bytes:
    /// the frame's revision repeats while its GEOMETRY changes. The cached
    /// preview has to pick the new layout up anyway, or the lamps keep
    /// drawing at their old offsets.
    #[test]
    fn a_moved_layout_reaches_a_frame_whose_revision_did_not_move() {
        let mut cache = OutputFrameCache::default();
        let mut first = entry(4, 5, vec![1, 0, 2, 0, 3, 0]);
        first.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        cache.apply(&[first]);

        let mut repatched = entry(4, 5, vec![1, 0, 2, 0, 3, 0]);
        repatched.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(12));
        cache.apply(&[repatched]);

        let ControlDisplayLayout::Layout2d(layout) = cache
            .frame(NodeId::new(4))
            .expect("frame")
            .display_layout
            .as_deref()
            .expect("layout");
        assert_eq!(layout.revision, Revision::new(12));
    }

    /// A patch edit re-cuts the wire without moving the frame's bytes — the
    /// same repeated-revision case the layout has to survive. The bay reads
    /// the cut, so it has to land whether or not the frame did.
    #[test]
    fn a_recut_wire_reaches_a_frame_whose_revision_did_not_move() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[entry(4, 5, vec![1, 0, 2, 0, 3, 0])]);
        assert_eq!(cache.placements(NodeId::new(4)).len(), 1);

        let mut repatched = entry(4, 5, vec![1, 0, 2, 0, 3, 0]);
        repatched.placements[0].reversed = true;
        cache.apply(&[repatched]);

        assert!(
            cache.placements(NodeId::new(4))[0].reversed,
            "the re-cut arrives under an unchanged frame revision"
        );
        assert_eq!(cache.outputs().collect::<Vec<_>>(), vec![NodeId::new(4)]);
    }

    #[test]
    fn a_refused_layout_stops_the_asking() {
        let mut cache = OutputFrameCache::default();
        let mut refused = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        refused.display_layout = ControlDisplayLayoutProbeResult::Unsupported {
            reason: "over the wire budget".to_string(),
        };
        cache.apply(&[refused]);

        assert_eq!(cache.display_layout_read(), ControlDisplayLayoutRead::None);
        assert!(
            cache
                .frame(NodeId::new(4))
                .expect("the bytes still arrived")
                .display_layout
                .is_none(),
            "no geometry is drawn rather than a guessed one"
        );
    }

    /// Two outputs, one still waiting: the shared gate has to keep asking
    /// outright, or the second output never gets geometry at all.
    #[test]
    fn one_output_without_geometry_keeps_the_shared_gate_open() {
        let mut cache = OutputFrameCache::default();
        let mut answered = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        answered.display_layout = ControlDisplayLayoutProbeResult::Layout(layout(11));
        let waiting = entry(5, 1, vec![1, 0, 2, 0, 3, 0]);
        cache.apply(&[answered, waiting]);

        assert_eq!(
            cache.display_layout_read(),
            ControlDisplayLayoutRead::Always
        );
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
            // One auto-flowed producer (a fixture, not the output itself)
            // taking the whole wire — the shape of an unpatched project.
            placements: vec![WireOutputPlacement {
                node: NodeId::new(node + 100),
                output: 0,
                source_lamp: 0,
                source_lamps: (bytes.len() / 6) as u32,
                wire_lamp: 0,
                lamps: (bytes.len() / 6) as u32,
                reversed: false,
            }],
            bytes,
        }
    }

    fn layout(revision: i64) -> ControlDisplayLayout {
        ControlDisplayLayout::Layout2d(ControlLayout2d::new(
            Revision::new(revision),
            8,
            8,
            vec![ControlLamp2d {
                lamp_index: 0,
                sample_start: 0,
                center: [0.5, 0.5],
                radius: 0.05,
            }],
        ))
    }
}

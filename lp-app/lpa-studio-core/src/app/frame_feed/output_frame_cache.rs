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
//! - **Geometry is cached per output, and samples are drawn only against
//!   it.** The probe's geometry bundle (sample layout, display layout,
//!   placements — `lpc_wire::OutputFrameGeometry`) arrives once and then
//!   answers `Unchanged` under its revision. The rules:
//!   - `Changed` replaces the output's cached geometry;
//!   - `Unchanged` keeps it (a revision this cache does not hold drops it);
//!   - `Omitted` drops it;
//!   - samples for an output with no cached geometry are NOT drawn — the
//!     last good frame stays — and the next read asks for that output's
//!     geometry outright ([`OutputFrameCache::geometry_read`] leaves it off
//!     the known list).
//!
//!   A refused display layout (`Unsupported`) is cached like any other
//!   answer: the lens keeps drawing samples against the cached sample layout
//!   with no lamp positions, and the engine answers `Unchanged` instead of
//!   rebuilding and re-measuring the refusal every read. Dropping the cache
//!   on a reconnect or project switch is explicit: the owners replace this
//!   whole cache, or call [`OutputFrameCache::forget_geometry`].
//! - **The `LampView` `Rc` contract.** The renderer repaints on `Rc` POINTER
//!   identity, so a genuinely new frame gets a FRESH `bytes` `Rc` while the
//!   layout keeps a STABLE one across frames whose geometry did not move —
//!   which is why the layout is cached per output rather than rebuilt into
//!   each preview.
//! - **Revision-only change detection.** A repeated revision leaves the
//!   cached preview, and its `Rc`s, exactly as they were.
//!
//! The device card's feed (`crate::app::frame_feed::card_feed`) and the
//! preview host's (`crate::app::preview_host::preview_output_feed`) keep the
//! same guarantees for their own transports; this one owns no pacing, no
//! connection, and no fallback, because the lens read already has all three.
//!
//! [`ProjectSync`]: super::project_sync::ProjectSync

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlPathSpan2d,
    ControlSampleLayout, ControlSampleSpan, NodeId, Revision,
};
use lpc_wire::{
    KnownOutputFrameGeometry, OutputFrameEntry, OutputFrameGeometry, OutputFrameGeometryRead,
    RevisionGateResult, WireOutputPlacement,
};

use crate::{UiControlProductPreview, UiControlSampleFormat};

/// One output node's cached published frame.
#[derive(Debug, Default)]
struct OutputFrameEntryState {
    /// The geometry the engine last sent for this output, while its revision
    /// stands. `None` until the first `Changed`, and again after an
    /// `Omitted` or an `Unchanged` naming a revision this cache does not
    /// hold — see the module docs.
    geometry: Option<CachedOutputGeometry>,
    /// The newest frame, as the renderer consumes it.
    frame: Option<UiControlProductPreview>,
}

/// One output's geometry bundle, as the lens keeps it between reads.
#[derive(Debug)]
struct CachedOutputGeometry {
    /// The engine-side revision of the whole bundle, for `IfChanged` gating.
    revision: Revision,
    /// How the output's samples group into lamps.
    sample_layout: ControlSampleLayout,
    /// The geometry every preview of this output shares — one `Rc`, replaced
    /// only when the engine sends a new bundle. `None` when the engine
    /// refused the display layout at this revision (dome-scale layouts are
    /// over the serialized-size budget): the refusal is cached too, so the
    /// engine is not asked to rebuild and re-measure it every read.
    layout: Option<Rc<ControlDisplayLayout>>,
    /// How the wire is CUT: one run per producer placed on this wire. Kept
    /// beside the frame rather than folded into it because it is not
    /// something the lamp renderer reads — it is what the patch bay draws,
    /// and the only description of which fixture owns which stretch of the
    /// strand (D34a). A patch edit re-cuts the wire without republishing
    /// bytes; it moves the bundle's revision, which is how it arrives.
    placements: Vec<WireOutputPlacement>,
}

impl CachedOutputGeometry {
    fn from_wire(geometry: &OutputFrameGeometry) -> Self {
        Self {
            revision: geometry.revision,
            sample_layout: geometry.sample_layout.clone(),
            layout: geometry
                .display_layout
                .layout()
                .map(|layout| Rc::new(layout.clone())),
            placements: geometry.placements.clone(),
        }
    }
}

/// The cached composed picture — see [`OutputFrameCache::composed_frame`].
///
/// Identity is `Rc` POINTER pairs, not contents, exactly like the renderer's
/// own paint key: the composite is rebuilt only when a part's bytes or
/// geometry `Rc` actually moved, and the two halves are keyed separately so
/// unchanged geometry keeps its `Rc` (the renderer's whole-field repaint
/// key) while fresh bytes arrive every frame.
#[derive(Debug)]
struct ComposedSlot {
    /// `(node, Rc::as_ptr(bytes))` per part, in compose order.
    bytes_key: Vec<(NodeId, usize)>,
    /// `(node, Rc::as_ptr(layout))` per part (0 for a layout-less part).
    layout_key: Vec<(NodeId, usize)>,
    frame: UiControlProductPreview,
}

/// Published output frames for the lens, keyed by output node.
#[derive(Debug, Default)]
pub struct OutputFrameCache {
    outputs: BTreeMap<NodeId, OutputFrameEntryState>,
    frames_seen: u64,
    /// One cached composite (interior-mutable: composing is a READ of the
    /// cache, and every caller holds `&self`).
    composed: RefCell<Option<ComposedSlot>>,
}

impl OutputFrameCache {
    /// How many times a NEW frame has landed here — the lens's own frame
    /// clock, and the only honest tick the controller has.
    ///
    /// Bumped once per probe answer that carried a moved revision on any
    /// output, so it counts engine frames rather than reads: a repeated
    /// revision (a patch edit re-cutting the wire without republishing) is
    /// not a frame. Zero means no frame has EVER arrived, which is the
    /// state every story renders in — see
    /// [`super::patch_preview::preview_phase`].
    pub fn frames_seen(&self) -> u64 {
        self.frames_seen
    }

    /// The newest frame one output published, if a read has carried one.
    pub fn frame(&self, node: NodeId) -> Option<&UiControlProductPreview> {
        self.outputs.get(&node)?.frame.as_ref()
    }

    /// How that output's wire is cut — its runs, in planning order. Empty
    /// for an output that has not answered, or has not planned yet.
    pub fn placements(&self, node: NodeId) -> &[WireOutputPlacement] {
        self.outputs
            .get(&node)
            .and_then(|output| output.geometry.as_ref())
            .map_or(&[], |geometry| geometry.placements.as_slice())
    }

    /// Every output that has answered a probe, in node order — the set the
    /// patch bay walks (a fixture's cells may be on any of them).
    pub fn outputs(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.outputs.keys().copied()
    }

    /// The picture `nodes` compose TOGETHER: every listed output's newest
    /// frame concatenated into one buffer, with each part's lamps rebased
    /// onto its stretch of it.
    ///
    /// This is the multi-output twin of the engine's own
    /// `merge_fragment_display_layouts`, one level up: the engine merges the
    /// producers WITHIN one wire (their layouts share the mapping document's
    /// normalized space, which is what makes overlaying them a picture), and
    /// exactly the same property holds ACROSS wires cut from the same
    /// project — the small dome's two boxes each carry half of one dome.
    /// Without this step a multi-output module drew only whichever output a
    /// consumer picked first, which is how the small dome's Box 2 (2,975
    /// lamps) never appeared in any lamp view.
    ///
    /// Contract kept for consumers:
    /// - **One part passes through untouched** — same `Rc`s, so the
    ///   single-output world is byte-for-byte what it was.
    /// - **Unchanged inputs answer the cached composite** — same `Rc`s
    ///   again, so a poll that changed nothing repaints nothing.
    /// - **Unmoved geometry keeps its `Rc`** while fresh bytes arrive: the
    ///   composed layout is rebuilt only when a part's layout `Rc` moved.
    ///
    /// A part with no layout still contributes its BYTES (its stretch of
    /// the buffer keeps every other part's offsets honest); it simply draws
    /// no lamps, which is the same honest answer the single-output path
    /// gives. `None` when no listed output has a frame at all.
    pub fn composed_frame(&self, nodes: &[NodeId]) -> Option<UiControlProductPreview> {
        let parts: Vec<(NodeId, &UiControlProductPreview)> = nodes
            .iter()
            .filter_map(|node| {
                let frame = self.outputs.get(node)?.frame.as_ref()?;
                Some((*node, frame))
            })
            .collect();
        let [first, rest @ ..] = parts.as_slice() else {
            return None;
        };
        if rest.is_empty() {
            return Some(first.1.clone());
        }

        let bytes_key: Vec<(NodeId, usize)> = parts
            .iter()
            .map(|(node, frame)| (*node, Rc::as_ptr(&frame.bytes) as *const u8 as usize))
            .collect();
        let layout_key: Vec<(NodeId, usize)> = parts
            .iter()
            .map(|(node, frame)| {
                let layout = frame
                    .display_layout
                    .as_ref()
                    .map_or(0, |layout| Rc::as_ptr(layout) as usize);
                (*node, layout)
            })
            .collect();
        let mut slot = self.composed.borrow_mut();
        if let Some(cached) = slot.as_ref()
            && cached.bytes_key == bytes_key
            && cached.layout_key == layout_key
        {
            return Some(cached.frame.clone());
        }
        let display_layout = match slot
            .as_ref()
            .filter(|cached| cached.layout_key == layout_key)
        {
            Some(cached) => cached.frame.display_layout.clone(),
            None => compose_display_layout(&parts),
        };
        let frame = compose_frame(&parts, display_layout);
        *slot = Some(ComposedSlot {
            bytes_key,
            layout_key,
            frame: frame.clone(),
        });
        Some(frame)
    }

    /// What the next read should ask for: every output whose geometry this
    /// cache holds, with the revision it holds it at. An output left off the
    /// list — never seen, or dropped — gets its geometry outright, which is
    /// how a new output or a deferred one asks `Always` while its neighbours
    /// answer `Unchanged`. `Always` while nothing is held at all.
    pub fn geometry_read(&self) -> OutputFrameGeometryRead {
        let known: Vec<KnownOutputFrameGeometry> = self
            .outputs
            .iter()
            .filter_map(|(node, output)| {
                output
                    .geometry
                    .as_ref()
                    .map(|geometry| KnownOutputFrameGeometry {
                        node: *node,
                        revision: geometry.revision,
                    })
            })
            .collect();
        if known.is_empty() {
            return OutputFrameGeometryRead::Always;
        }
        OutputFrameGeometryRead::IfChanged { known }
    }

    /// Drop every output's cached geometry, keeping the frames: the next read
    /// asks for all of it again. For a caller whose claims about the device
    /// stopped holding (a malformed read stream) while the pictures it
    /// already drew are still the last honest ones.
    pub fn forget_geometry(&mut self) {
        for output in self.outputs.values_mut() {
            output.geometry = None;
        }
    }

    /// Fold one probe answer — every output entry it carried — into the cache.
    pub fn apply(&mut self, outputs: &[OutputFrameEntry]) {
        let mut moved = false;
        for entry in outputs {
            moved |= self.apply_entry(entry);
        }
        if moved {
            self.frames_seen = self.frames_seen.wrapping_add(1);
        }
    }

    /// Returns whether this entry carried a NEW frame (a moved revision with
    /// readable bytes, drawn against cached geometry) — what
    /// [`Self::frames_seen`] counts.
    fn apply_entry(&mut self, entry: &OutputFrameEntry) -> bool {
        let output = self.outputs.entry(entry.node).or_default();
        let geometry_moved = match &entry.geometry {
            RevisionGateResult::Changed(geometry) => {
                output.geometry = Some(CachedOutputGeometry::from_wire(geometry));
                true
            }
            // "What you have still stands" — keep the `Rc`s, which is the
            // whole point of asking `IfChanged`. A revision this cache does
            // not hold cannot stand for anything: drop it and ask again.
            RevisionGateResult::Unchanged { revision } => {
                if output
                    .geometry
                    .as_ref()
                    .is_none_or(|geometry| geometry.revision != *revision)
                {
                    output.geometry = None;
                }
                false
            }
            // No statement this read (the engine deferred it, or has none
            // yet): nothing cached can be trusted to match these samples.
            RevisionGateResult::Omitted => {
                output.geometry = None;
                false
            }
        };
        // No geometry, no drawing: the last good frame stays up rather than
        // new samples being laid out against a guess. The next read asks.
        let Some(geometry) = output.geometry.as_ref() else {
            return false;
        };

        let unchanged = output
            .frame
            .as_ref()
            .is_some_and(|frame| frame.revision == entry.revision.0);
        // No samples (the read asked for geometry only), or a buffer whose
        // length does not match its own format: nothing to draw. Keep the
        // last good frame instead.
        let format = entry.sample_format.map(UiControlSampleFormat::from_wire);
        let readable = format.is_some_and(|format| {
            entry.channels > 0
                && entry.bytes.len() == entry.channels as usize * 3 * format.bytes_per_sample()
        });
        if unchanged {
            // Geometry may still have moved under a frame that did not: a
            // patch edit re-places the wire without republishing bytes.
            if geometry_moved && let Some(frame) = output.frame.as_mut() {
                frame.sample_layout = geometry.sample_layout.clone();
                frame.display_layout = geometry.layout.clone();
            }
            return false;
        }
        let (true, Some(format)) = (readable, format) else {
            return false;
        };
        output.frame = Some(UiControlProductPreview {
            revision: entry.revision.0,
            // The published buffer is one row of RGB samples: three
            // channels per lamp.
            extent: ControlExtent::new(1, entry.channels.saturating_mul(3)),
            sample_format: format,
            sample_layout: geometry.sample_layout.clone(),
            // The ONE geometry Rc, cloned — pointer-stable across frames.
            display_layout: geometry.layout.clone(),
            // A fresh Rc per frame: the renderer's repaint key.
            bytes: Rc::from(entry.bytes.as_slice()),
        });
        true
    }
}

/// Concatenate the parts' buffers and spans into one frame, in the order
/// given. Each part's samples land at a running offset, and the composed
/// spans move with them, so a consumer reading a lamp's colour at its
/// `sample_start` finds exactly that part's bytes.
///
/// Parts that all arrived in one format concatenate as they are; a mix (an
/// output that publishes 8-bit beside ones that publish 16) widens every
/// part to `U16`, the format no sample loses precision in.
fn compose_frame(
    parts: &[(NodeId, &UiControlProductPreview)],
    display_layout: Option<Rc<ControlDisplayLayout>>,
) -> UiControlProductPreview {
    let first_format = parts
        .first()
        .map_or(UiControlSampleFormat::U16, |(_, part)| part.sample_format);
    let sample_format = if parts
        .iter()
        .all(|(_, part)| part.sample_format == first_format)
    {
        first_format
    } else {
        UiControlSampleFormat::U16
    };
    let mut bytes: Vec<u8> = Vec::with_capacity(
        parts
            .iter()
            .map(|(_, frame)| frame.extent.sample_count() as usize)
            .sum::<usize>()
            * sample_format.bytes_per_sample(),
    );
    let mut spans: Vec<ControlSampleSpan> = Vec::new();
    let mut revision = i64::MIN;
    let mut offset_samples = 0u32;
    for (_, part) in parts {
        revision = revision.max(part.revision);
        for span in &part.sample_layout.spans {
            spans.push(ControlSampleSpan {
                row: span.row,
                start: span.start.saturating_add(offset_samples),
                len: span.len,
                encoding: span.encoding.clone(),
            });
        }
        if part.sample_format == sample_format {
            bytes.extend_from_slice(&part.bytes);
        } else {
            for index in 0..part.extent.sample_count() as usize {
                let sample = part.unorm16_sample(index).unwrap_or(0);
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        offset_samples = offset_samples.saturating_add(part.extent.sample_count());
    }
    UiControlProductPreview {
        revision,
        extent: ControlExtent::new(1, offset_samples),
        sample_format,
        sample_layout: ControlSampleLayout { spans },
        display_layout,
        bytes: Rc::from(bytes.as_slice()),
    }
}

/// Overlay the parts' geometries, each lamp rebased by its part's sample
/// offset in the composed buffer. Normalized centers overlay as they are —
/// the same shared-document-space property the engine's per-wire merge
/// already relies on. `None` when no part carries geometry.
fn compose_display_layout(
    parts: &[(NodeId, &UiControlProductPreview)],
) -> Option<Rc<ControlDisplayLayout>> {
    let mut lamps: Vec<ControlLamp2d> = Vec::new();
    let mut paths: Vec<ControlPathSpan2d> = Vec::new();
    let mut width_hint = 0;
    let mut height_hint = 0;
    let mut revision = Revision::default();
    let mut any = false;
    let mut offset_samples = 0u32;
    for (_, part) in parts {
        if let Some(layout) = &part.display_layout {
            let ControlDisplayLayout::Layout2d(layout) = layout.as_ref();
            any = true;
            width_hint = width_hint.max(layout.width_hint);
            height_hint = height_hint.max(layout.height_hint);
            revision = revision.max(layout.revision);
            for lamp in &layout.lamps {
                let sample_start = lamp.sample_start.saturating_add(offset_samples);
                lamps.push(ControlLamp2d {
                    lamp_index: sample_start / 3,
                    sample_start,
                    center: lamp.center,
                    radius: lamp.radius,
                });
            }
            for path in &layout.paths {
                paths.push(ControlPathSpan2d {
                    first_lamp: path.first_lamp.saturating_add(offset_samples / 3),
                    lamp_count: path.lamp_count,
                });
            }
        }
        offset_samples = offset_samples.saturating_add(part.extent.sample_count());
    }
    any.then(|| {
        // Composed-buffer order, matching the engine's own merged answer.
        lamps.sort_by_key(|lamp| lamp.sample_start);
        paths.sort_by_key(|path| path.first_lamp);
        Rc::new(ControlDisplayLayout::Layout2d(
            ControlLayout2d::new(revision, width_hint, height_hint, lamps).with_paths(paths),
        ))
    })
}

#[cfg(test)]
mod tests {
    use lpc_model::{ColorOrder, ControlSampleEncoding};
    use lpc_wire::{GeometryDisplayLayout, WireChannelSampleFormat};

    use super::*;

    #[test]
    fn the_first_read_asks_always_and_later_ones_list_what_they_hold() {
        let mut cache = OutputFrameCache::default();
        assert_eq!(cache.geometry_read(), OutputFrameGeometryRead::Always);

        cache.apply(&[with_layout(
            entry(4, 1, vec![1, 0, 2, 0, 3, 0]),
            11,
            layout(11),
        )]);

        assert_eq!(
            cache.geometry_read(),
            OutputFrameGeometryRead::IfChanged {
                known: vec![known(4, 11)],
            }
        );
    }

    /// The layout `Rc` is the renderer's repaint key for the whole lamp
    /// field: an `Unchanged` answer must hand back the SAME one — and the
    /// samples still draw, against the cached sample layout.
    #[test]
    fn unchanged_geometry_keeps_its_rc_while_new_bytes_arrive() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[with_layout(
            entry(4, 1, vec![1, 0, 2, 0, 3, 0]),
            11,
            layout(11),
        )]);
        let geometry = Rc::clone(
            cache
                .frame(NodeId::new(4))
                .expect("frame")
                .display_layout
                .as_ref()
                .expect("layout"),
        );
        let bytes = Rc::clone(&cache.frame(NodeId::new(4)).expect("frame").bytes);

        cache.apply(&[unchanged(entry(4, 2, vec![9, 0, 8, 0, 7, 0]), 11)]);

        let frame = cache.frame(NodeId::new(4)).expect("frame");
        assert!(Rc::ptr_eq(
            &geometry,
            frame.display_layout.as_ref().expect("layout")
        ));
        assert!(
            !Rc::ptr_eq(&bytes, &frame.bytes),
            "a new frame must carry a fresh bytes Rc"
        );
        assert_eq!(frame.bytes.as_ref(), [9, 0, 8, 0, 7, 0]);
        assert_eq!(frame.sample_layout.spans.len(), 1, "drawn from the cache");
    }

    /// A patch edit re-places the wire without moving the published bytes:
    /// the frame's revision repeats while its GEOMETRY changes. The cached
    /// preview has to pick the new layout up anyway, or the lamps keep
    /// drawing at their old offsets.
    #[test]
    fn a_moved_layout_reaches_a_frame_whose_revision_did_not_move() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[with_layout(
            entry(4, 5, vec![1, 0, 2, 0, 3, 0]),
            11,
            layout(11),
        )]);
        cache.apply(&[with_layout(
            entry(4, 5, vec![1, 0, 2, 0, 3, 0]),
            12,
            layout(12),
        )]);

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
        let RevisionGateResult::Changed(geometry) = &mut repatched.geometry else {
            unreachable!("the helper sends geometry");
        };
        geometry.revision = Revision::new(2);
        geometry.placements[0].reversed = true;
        cache.apply(&[repatched]);

        assert!(
            cache.placements(NodeId::new(4))[0].reversed,
            "the re-cut arrives under an unchanged frame revision"
        );
        assert_eq!(cache.outputs().collect::<Vec<_>>(), vec![NodeId::new(4)]);
    }

    /// A refused display layout is an ANSWER, cached under its revision
    /// like a layout: the next read lists it (so the engine says
    /// `Unchanged` instead of rebuilding and re-measuring the refusal), and
    /// the frame still draws its samples — with no lamp positions rather
    /// than guessed ones.
    #[test]
    fn a_refused_layout_is_cached_under_its_revision() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[entry(4, 1, vec![1, 0, 2, 0, 3, 0])]);

        assert_eq!(
            cache.geometry_read(),
            OutputFrameGeometryRead::IfChanged {
                known: vec![known(4, GEOMETRY_REVISION)],
            },
            "the refusal is held, not re-asked"
        );
        let frame = cache
            .frame(NodeId::new(4))
            .expect("the bytes still arrived");
        assert!(
            frame.display_layout.is_none(),
            "no geometry is drawn rather than a guessed one"
        );

        cache.apply(&[unchanged(
            entry(4, 2, vec![4, 0, 5, 0, 6, 0]),
            GEOMETRY_REVISION,
        )]);
        assert_eq!(
            cache.frame(NodeId::new(4)).expect("frame").bytes.as_ref(),
            [4, 0, 5, 0, 6, 0],
            "steady samples keep flowing under the cached refusal"
        );
    }

    /// Samples for an output with no cached geometry — a new output, a
    /// reconnect, a deferred bundle — are not drawn, and the next read asks
    /// for that output's geometry outright (it is left off the known list)
    /// while its neighbours stay gated.
    #[test]
    fn samples_without_cached_geometry_are_not_drawn_and_are_asked_for_again() {
        let mut cache = OutputFrameCache::default();
        let answered = with_layout(entry(4, 1, vec![1, 0, 2, 0, 3, 0]), 11, layout(11));
        let mut deferred = entry(5, 1, vec![9, 0, 8, 0, 7, 0]);
        deferred.geometry = RevisionGateResult::Omitted;
        cache.apply(&[answered, deferred]);

        assert!(
            cache.frame(NodeId::new(5)).is_none(),
            "no geometry, no picture — not a guessed one"
        );
        assert_eq!(
            cache.geometry_read(),
            OutputFrameGeometryRead::IfChanged {
                known: vec![known(4, 11)],
            },
            "output 5 is off the list, so the engine sends its geometry"
        );

        // An `Unchanged` naming a revision this cache never held is no
        // better than nothing: it is dropped and asked for again too.
        cache.apply(&[unchanged(entry(4, 2, vec![3, 0, 3, 0, 3, 0]), 99)]);
        assert_eq!(cache.geometry_read(), OutputFrameGeometryRead::Always);
        assert_eq!(
            cache
                .frame(NodeId::new(4))
                .expect("last good frame")
                .bytes
                .as_ref(),
            [1, 0, 2, 0, 3, 0],
            "the last good frame stays up"
        );
    }

    /// Forgetting is explicit, and keeps the pictures: a caller whose claims
    /// stopped holding asks for every output's geometry again.
    #[test]
    fn forgetting_geometry_asks_again_and_keeps_the_frames() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[with_layout(
            entry(4, 1, vec![1, 0, 2, 0, 3, 0]),
            11,
            layout(11),
        )]);

        cache.forget_geometry();

        assert_eq!(cache.geometry_read(), OutputFrameGeometryRead::Always);
        assert!(cache.frame(NodeId::new(4)).is_some());
    }

    /// The lens's frame clock: it counts ENGINE frames, not reads. A
    /// repeated revision (a patch edit re-cutting the wire under unchanged
    /// bytes) must not advance it, or the unmapped-chase preview would
    /// animate while nothing is playing — and stories would stop being
    /// deterministic.
    #[test]
    fn frames_seen_counts_moved_revisions_only() {
        let mut cache = OutputFrameCache::default();
        assert_eq!(cache.frames_seen(), 0, "no frame has ever arrived");

        cache.apply(&[entry(4, 1, vec![1, 0, 2, 0, 3, 0])]);
        assert_eq!(cache.frames_seen(), 1);

        cache.apply(&[entry(4, 1, vec![1, 0, 2, 0, 3, 0])]);
        assert_eq!(cache.frames_seen(), 1, "a repeated revision is not a frame");

        cache.apply(&[entry(4, 2, vec![9, 0, 8, 0, 7, 0])]);
        assert_eq!(cache.frames_seen(), 2);

        // Two outputs moving together are ONE frame of the show, not two.
        cache.apply(&[
            entry(4, 3, vec![9, 0, 8, 0, 7, 0]),
            entry(5, 3, vec![1, 0, 1, 0, 1, 0]),
        ]);
        assert_eq!(cache.frames_seen(), 3);

        // Unreadable bytes (a length that contradicts the format: six
        // bytes cannot be one lamp at 8 bits) leave the last good frame —
        // and the clock.
        let mut garbage = entry(4, 4, vec![9, 0, 8, 0, 7, 0]);
        garbage.sample_format = Some(WireChannelSampleFormat::U8);
        cache.apply(&[garbage]);
        assert_eq!(cache.frames_seen(), 3);
    }

    /// An 8-bit frame draws as one: the preview keeps the format it arrived
    /// in, and the shared decode widens each level to unorm16.
    #[test]
    fn an_eight_bit_frame_draws_as_eight_bit() {
        let mut cache = OutputFrameCache::default();
        let mut narrow = entry(4, 1, vec![0; 6]);
        narrow.sample_format = Some(WireChannelSampleFormat::U8);
        narrow.bytes = vec![255, 128, 0];

        cache.apply(&[narrow]);

        let frame = cache.frame(NodeId::new(4)).expect("an 8-bit frame draws");
        assert_eq!(frame.sample_format, UiControlSampleFormat::U8);
        assert_eq!(frame.extent.sample_count(), 3);
        assert_eq!(frame.unorm16_sample(0), Some(u16::MAX));
        assert_eq!(frame.unorm16_sample(1), Some(128 * 257));
        assert_eq!(cache.frames_seen(), 1);
    }

    /// A geometry-only answer (the read asked for no samples) still caches
    /// the geometry — placements included — and leaves the last frame and
    /// the frame clock alone.
    #[test]
    fn a_samples_free_answer_keeps_geometry_and_the_last_frame() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[entry(4, 1, vec![1, 0, 2, 0, 3, 0])]);

        let mut bare = entry(4, 2, vec![9, 0, 8, 0, 7, 0]);
        bare.sample_format = None;
        bare.bytes = Vec::new();
        cache.apply(&[bare]);

        assert_eq!(cache.placements(NodeId::new(4)).len(), 1);
        assert_eq!(
            cache.frame(NodeId::new(4)).expect("last frame").revision,
            1,
            "no samples, no new picture"
        );
        assert_eq!(cache.frames_seen(), 1);
    }

    /// Outputs that arrived in different formats compose at `U16`, each
    /// 8-bit level widened by ×257, so no part loses precision.
    #[test]
    fn mixed_formats_compose_widened_to_sixteen_bits() {
        let mut cache = OutputFrameCache::default();
        let wide = with_layout(entry(4, 1, vec![1, 0, 2, 0, 3, 0]), 11, layout_at(11, 0.25));
        let mut narrow = with_layout(entry(5, 1, vec![0; 6]), 12, layout_at(12, 0.75));
        narrow.sample_format = Some(WireChannelSampleFormat::U8);
        narrow.bytes = vec![1, 2, 255];
        cache.apply(&[wide, narrow]);

        let composed = cache
            .composed_frame(&[NodeId::new(4), NodeId::new(5)])
            .expect("composed");

        assert_eq!(composed.sample_format, UiControlSampleFormat::U16);
        assert_eq!(
            composed.bytes.as_ref(),
            [1, 0, 2, 0, 3, 0, 1, 1, 2, 2, 255, 255]
        );
    }

    /// The small-dome regression (2026-08-29): TWO outputs cut from one
    /// module, both answering full frames and geometry — the composed
    /// picture must carry BOTH outputs' lamps, each reading its own bytes.
    /// Before the compose step every consumer picked one output and the
    /// second box's lamps never appeared anywhere.
    #[test]
    fn two_outputs_compose_into_one_picture_with_rebased_lamps() {
        let mut cache = OutputFrameCache::default();
        let box_1 = with_layout(entry(4, 1, vec![1, 0, 2, 0, 3, 0]), 11, layout_at(11, 0.25));
        let box_2 = with_layout(entry(5, 1, vec![9, 0, 8, 0, 7, 0]), 12, layout_at(12, 0.75));
        cache.apply(&[box_1, box_2]);

        let composed = cache
            .composed_frame(&[NodeId::new(4), NodeId::new(5)])
            .expect("both outputs have frames");

        assert_eq!(
            composed.bytes.as_ref(),
            [1, 0, 2, 0, 3, 0, 9, 0, 8, 0, 7, 0],
            "part buffers concatenate in the order given"
        );
        assert_eq!(composed.extent, ControlExtent::new(1, 6));
        let ControlDisplayLayout::Layout2d(layout) =
            composed.display_layout.as_deref().expect("geometry");
        assert_eq!(layout.revision, Revision::new(12), "newest part wins");
        let starts: Vec<u32> = layout.lamps.iter().map(|lamp| lamp.sample_start).collect();
        assert_eq!(
            starts,
            vec![0, 3],
            "the second output's lamp reads ITS stretch of the buffer"
        );
        assert_eq!(layout.lamps[1].center, [0.75, 0.75]);
        let spans: Vec<u32> = composed
            .sample_layout
            .spans
            .iter()
            .map(|span| span.start)
            .collect();
        assert_eq!(spans, vec![0, 3], "spans rebase with their part");
    }

    /// One part passes through untouched — the single-output world keeps
    /// its exact `Rc`s (the renderer's repaint keys).
    #[test]
    fn a_single_part_composes_as_itself() {
        let mut cache = OutputFrameCache::default();
        cache.apply(&[with_layout(
            entry(4, 1, vec![1, 0, 2, 0, 3, 0]),
            11,
            layout(11),
        )]);

        let composed = cache
            .composed_frame(&[NodeId::new(4)])
            .expect("the part composes");
        let part = cache.frame(NodeId::new(4)).expect("part");
        assert!(Rc::ptr_eq(&composed.bytes, &part.bytes));
        assert!(Rc::ptr_eq(
            composed.display_layout.as_ref().expect("layout"),
            part.display_layout.as_ref().expect("layout"),
        ));
    }

    /// New bytes with unmoved geometry rebuild the composed BYTES only:
    /// the composed layout keeps its `Rc`, which is what keeps the lamp
    /// renderer from rebuilding the whole cell field every live frame.
    #[test]
    fn unmoved_geometry_keeps_the_composed_layout_rc_across_new_bytes() {
        let mut cache = OutputFrameCache::default();
        let box_1 = with_layout(entry(4, 1, vec![1, 0, 2, 0, 3, 0]), 11, layout_at(11, 0.25));
        let box_2 = with_layout(entry(5, 1, vec![9, 0, 8, 0, 7, 0]), 12, layout_at(12, 0.75));
        cache.apply(&[box_1, box_2]);
        let nodes = [NodeId::new(4), NodeId::new(5)];
        let first = cache.composed_frame(&nodes).expect("composed");

        // Same inputs: the whole cached composite answers, Rcs and all.
        let repeat = cache.composed_frame(&nodes).expect("composed");
        assert!(Rc::ptr_eq(&first.bytes, &repeat.bytes));

        // A new frame on one output: fresh bytes, same geometry Rc.
        cache.apply(&[unchanged(entry(4, 2, vec![4, 0, 5, 0, 6, 0]), 11)]);
        let second = cache.composed_frame(&nodes).expect("composed");
        assert!(
            !Rc::ptr_eq(&first.bytes, &second.bytes),
            "a new frame must carry fresh composed bytes"
        );
        assert!(
            Rc::ptr_eq(
                first.display_layout.as_ref().expect("layout"),
                second.display_layout.as_ref().expect("layout"),
            ),
            "unmoved geometry keeps the composed layout Rc"
        );
        assert_eq!(&second.bytes[..6], [4, 0, 5, 0, 6, 0]);
    }

    /// A part the engine refused geometry for still holds its stretch of
    /// the composed buffer, so the parts that DID answer keep drawing at
    /// honest offsets.
    #[test]
    fn a_layout_less_part_keeps_its_neighbours_offsets_honest() {
        let mut cache = OutputFrameCache::default();
        let refused = entry(4, 1, vec![1, 0, 2, 0, 3, 0]);
        let answered = with_layout(entry(5, 1, vec![9, 0, 8, 0, 7, 0]), 12, layout_at(12, 0.75));
        cache.apply(&[refused, answered]);

        let composed = cache
            .composed_frame(&[NodeId::new(4), NodeId::new(5)])
            .expect("composed");
        let ControlDisplayLayout::Layout2d(layout) =
            composed.display_layout.as_deref().expect("geometry");
        assert_eq!(layout.lamps.len(), 1);
        assert_eq!(
            layout.lamps[0].sample_start, 3,
            "the answering part's lamp still reads its own stretch"
        );
        assert_eq!(composed.bytes.len(), 12);
    }

    /// The geometry revision [`entry`] sends by default.
    const GEOMETRY_REVISION: i64 = 1;

    fn known(node: u32, revision: i64) -> KnownOutputFrameGeometry {
        KnownOutputFrameGeometry {
            node: NodeId::new(node),
            revision: Revision::new(revision),
        }
    }

    /// The entry with a display layout in its geometry, at `revision`.
    fn with_layout(
        mut entry: OutputFrameEntry,
        revision: i64,
        layout: ControlDisplayLayout,
    ) -> OutputFrameEntry {
        let RevisionGateResult::Changed(geometry) = &mut entry.geometry else {
            unreachable!("the helper sends geometry");
        };
        geometry.revision = Revision::new(revision);
        geometry.display_layout = GeometryDisplayLayout::Layout(layout);
        entry
    }

    /// The entry answering `Unchanged` at `revision` instead of a bundle.
    fn unchanged(mut entry: OutputFrameEntry, revision: i64) -> OutputFrameEntry {
        entry.geometry = RevisionGateResult::Unchanged {
            revision: Revision::new(revision),
        };
        entry
    }

    fn layout_at(revision: i64, center: f32) -> ControlDisplayLayout {
        ControlDisplayLayout::Layout2d(
            ControlLayout2d::new(
                Revision::new(revision),
                8,
                8,
                vec![ControlLamp2d {
                    lamp_index: 0,
                    sample_start: 0,
                    center: [center, center],
                    radius: 0.05,
                }],
            )
            .with_paths(vec![ControlPathSpan2d {
                first_lamp: 0,
                lamp_count: 1,
            }]),
        )
    }

    /// A published frame with its whole geometry bundle: one span, one
    /// auto-flowed producer, and a REFUSED display layout (the bare case —
    /// [`with_layout`] adds one).
    fn entry(node: u32, revision: i64, bytes: Vec<u8>) -> OutputFrameEntry {
        OutputFrameEntry {
            node: NodeId::new(node),
            revision: Revision::new(revision),
            channels: (bytes.len() / 6) as u32,
            sample_format: Some(WireChannelSampleFormat::U16),
            geometry: RevisionGateResult::Changed(OutputFrameGeometry {
                revision: Revision::new(GEOMETRY_REVISION),
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
                display_layout: GeometryDisplayLayout::Unsupported {
                    reason: "no display layout in this fixture".to_string(),
                },
                // One auto-flowed producer (a fixture, not the output
                // itself) taking the whole wire — the shape of an unpatched
                // project.
                placements: vec![WireOutputPlacement {
                    node: NodeId::new(node + 100),
                    output: 0,
                    source_lamp: 0,
                    source_lamps: (bytes.len() / 6) as u32,
                    wire_lamp: 0,
                    lamps: (bytes.len() / 6) as u32,
                    reversed: false,
                }],
            }),
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

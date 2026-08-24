//! THE unmapped-selection chase, computed once — core-side (Q9).
//!
//! A MAPPED object's chase arrives as ordinary frame data: the engine paints
//! the light language into an output's published bytes before it publishes
//! (`lpc-engine`'s `paint_chase`), so the panel strip, the canvas sprites and
//! the physical piece all read one truth.
//!
//! An UNMAPPED object has no wire, so nothing publishes bytes for it — and it
//! is precisely the object a walk-up user is about to point at. Pass 2's
//! first cut let the panel strip compute that one chase for itself, which the
//! G1 gate rejected: "implies we're not driving those views from the same
//! data — that should be generated server side. very fishy". So it is
//! generated HERE, once, and every view consumes the same colors:
//! [`crate::UiPatchSurface::chase_preview`] reaches the panel strip and the
//! canvas's sprite live-fill feed together.
//!
//! The numbers are the ENGINE's, imported not copied
//! ([`lpc_model::nodes::output::chase`]) — two copies of the chase constants
//! is exactly the drift this module exists to kill. Colors come out in the
//! engine's own 16-bit linear space, so a client renders them through the
//! same linear → sRGB transfer it decodes any published frame with.

use lpc_model::NodeId;
use lpc_model::nodes::output::chase;

use crate::{UiPatchSurface, UiPatchSurfaceFixture, UiPatchTarget};

/// Published frames per full sweep of the preview's dot.
///
/// The wire states the chase's period in SECONDS ([`chase::SWEEP_SECONDS`]),
/// because the engine paints it against the frame clock it already holds.
/// The controller holds no clock — but it does see every published frame, and
/// a frame-counted sweep buys the one property a wall clock cannot: with no
/// frames flowing the preview FREEZES, which is what story capture's
/// never-widened thresholds require. Twenty-four frames is two seconds at the
/// studio's 12 fps default pacing, so panel and wall read as the same speed.
const PREVIEW_SWEEP_FRAMES: u64 = 24;

/// The still every un-fed preview stands at.
///
/// A quarter of the way along shows head, dot and tail at once — the most
/// legible single frame, and the same still pass 2's client chase froze at.
pub const FROZEN_PREVIEW_PHASE: f32 = 0.25;

/// The preview's phase after `frames_seen` engine frames (see
/// [`super::output_frame_cache::OutputFrameCache::frames_seen`]).
///
/// Zero frames = nothing has ever played here = the frozen still. Every
/// story renders in that state, and so does a project whose engine is idle.
#[must_use]
pub fn preview_phase(frames_seen: u64) -> f32 {
    if frames_seen == 0 {
        return FROZEN_PREVIEW_PHASE;
    }
    (frames_seen % PREVIEW_SWEEP_FRAMES) as f32 / PREVIEW_SWEEP_FRAMES as f32
}

/// The chase for the selected UNMAPPED object, in OBJECT order.
///
/// One truth for every view: the panel strip paints `colors` straight, and
/// the canvas paints the same colors onto the same object's sprite lamps
/// (`node` + `start` locate them in the fixture's OWN numbering — the space
/// `data-sprite-lamp` counts in).
#[derive(Clone, Debug, PartialEq)]
pub struct UiPatchChasePreview {
    /// The fixture the object belongs to.
    pub node: NodeId,
    /// The object's first lamp in the fixture's own numbering.
    pub start: u32,
    /// Object-order colors, 16-bit linear unorm RGB — one per lamp.
    pub colors: Vec<[u16; 3]>,
    /// The phase they were painted at (0..1), so a view can say whether it
    /// is watching a live sweep or the frozen still.
    pub phase: f32,
}

impl UiPatchChasePreview {
    /// Does this preview paint `node`'s lamp `lamp` (fixture numbering)?
    #[must_use]
    pub fn color_for(&self, node: NodeId, lamp: u32) -> Option<[u16; 3]> {
        if node != self.node {
            return None;
        }
        self.colors
            .get(lamp.checked_sub(self.start)? as usize)
            .copied()
    }
}

/// Compute the preview for `selection` on `surface`, or `None` when the
/// selection names no unmapped object (a mapped one already chases in its
/// published bytes; a wire-side or context selection is not an object at
/// all).
pub(crate) fn chase_preview(
    surface: &UiPatchSurface,
    selection: Option<&UiPatchTarget>,
    frames_seen: u64,
) -> Option<UiPatchChasePreview> {
    let (node, start, lamps) = unmapped_object_range(surface, selection?)?;
    if lamps == 0 {
        return None;
    }
    let phase = preview_phase(frames_seen);
    Some(UiPatchChasePreview {
        node,
        start,
        colors: (0..lamps)
            .map(|ordinal| chase::lamp_rgb_16(ordinal, lamps, phase))
            .collect(),
        phase,
    })
}

/// The `(fixture, start, lamps)` a fixture-side selection names, when NO run
/// places those lamps.
///
/// The mapped test is the surface's own runs (`patch.cells`), the same
/// derivation the tree's mapped/unmapped dot and the panel's `wire` fact
/// read: a partially placed range counts as mapped, because the part that IS
/// on a wire already carries the engine's chase and the two pictures must not
/// fight over the same object.
fn unmapped_object_range(
    surface: &UiPatchSurface,
    target: &UiPatchTarget,
) -> Option<(NodeId, u32, u32)> {
    let (fixture, start, lamps) = match target {
        UiPatchTarget::Instance { node, path } => {
            let fixture = fixture_of(surface, *node)?;
            let instance = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path)?;
            (fixture, instance.start, instance.lamps)
        }
        UiPatchTarget::Range { node, start, count } => {
            let fixture = fixture_of(surface, *node)?;
            let lamps = count.unwrap_or_else(|| fixture.patch.lamps.saturating_sub(*start));
            (fixture, *start, lamps)
        }
        // A `Cell` IS a run: it is mapped by construction. Wire-side and
        // context selections name no object — and neither does a whole
        // FIXTURE, whatever its object table looks like: since the round-3
        // rework a fixture target speaks BREATH (D9), so a chase painted
        // over its sprites would be the canvas contradicting the wire.
        _ => return None,
    };
    let end = start.saturating_add(lamps);
    let placed = fixture
        .patch
        .cells
        .iter()
        .any(|cell| cell.source_start < end && cell.source_start + cell.lamps > start);
    (!placed).then_some((fixture.node, start, lamps))
}

fn fixture_of(surface: &UiPatchSurface, node: NodeId) -> Option<&UiPatchSurfaceFixture> {
    surface.fixtures.iter().find(|fixture| fixture.node == node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiFixturePatch, UiPatchCell, UiPatchInstance, UiPatchSurfaceFixture};

    fn dome() -> NodeId {
        NodeId::new(2)
    }

    fn instance(path: &str, start: u32, lamps: u32) -> UiPatchInstance {
        UiPatchInstance {
            path: path.to_string(),
            label: path.to_string(),
            start,
            lamps,
            stride: 1,
            placed: false,
        }
    }

    /// Sector 1 on a wire, sector 2 still waiting — the shape both flows
    /// walk.
    fn half_patched() -> UiPatchSurface {
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: dome(),
                label: "dome".to_string(),
                patch: UiFixturePatch {
                    lamps: 60,
                    cells: vec![UiPatchCell {
                        id: "2:0".to_string(),
                        source_start: 0,
                        lamps: 30,
                        wire_start: 0,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                instances: vec![instance("/sector/1", 0, 30), instance("/sector/2", 30, 30)],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn sector(path: &str) -> UiPatchTarget {
        UiPatchTarget::Instance {
            node: dome(),
            path: path.to_string(),
        }
    }

    /// The preview is the ENGINE's language, lamp for lamp — asserted
    /// against the shared home rather than against copied literals, which is
    /// the whole point of Q9.
    #[test]
    fn the_preview_paints_the_shared_chase_in_object_order() {
        let surface = half_patched();
        let preview = chase_preview(&surface, Some(&sector("/sector/2")), 0).expect("preview");

        assert_eq!(preview.node, dome());
        assert_eq!(preview.start, 30, "object lamps in FIXTURE numbering");
        assert_eq!(preview.colors.len(), 30);
        assert_eq!(preview.phase, FROZEN_PREVIEW_PHASE);
        let expected: Vec<[u16; 3]> = (0..30)
            .map(|ordinal| chase::lamp_rgb_16(ordinal, 30, FROZEN_PREVIEW_PHASE))
            .collect();
        assert_eq!(preview.colors, expected);
        // …which is a blue head and a red tail, said out loud once.
        assert_eq!(preview.colors[0], chase::rgb8_to_16(chase::HEAD_RGB));
        assert_eq!(preview.colors[29], chase::rgb8_to_16(chase::TAIL_RGB));
    }

    /// The canvas asks lamp by lamp: the preview answers only for the lamps
    /// of the object it painted, on the fixture it belongs to.
    #[test]
    fn the_preview_answers_for_its_own_lamps_only() {
        let surface = half_patched();
        let preview = chase_preview(&surface, Some(&sector("/sector/2")), 0).expect("preview");

        assert_eq!(
            preview.color_for(dome(), 30),
            Some(chase::rgb8_to_16(chase::HEAD_RGB)),
        );
        assert_eq!(preview.color_for(dome(), 29), None, "before the object");
        assert_eq!(preview.color_for(dome(), 60), None, "past its last lamp");
        assert_eq!(
            preview.color_for(NodeId::new(9), 30),
            None,
            "another fixture's lamp 30 is not this object's",
        );
    }

    /// Only an UNMAPPED object gets a preview: a mapped one already chases
    /// in the bytes its output published, and painting a second chase over
    /// it would be two pictures of one object.
    #[test]
    fn a_mapped_selection_gets_no_preview() {
        let surface = half_patched();
        assert!(chase_preview(&surface, Some(&sector("/sector/1")), 0).is_none());
        assert!(
            chase_preview(
                &surface,
                Some(&UiPatchTarget::Cell {
                    id: "2:0".to_string(),
                }),
                0,
            )
            .is_none(),
            "a cell IS a run",
        );
        assert!(
            chase_preview(
                &surface,
                Some(&UiPatchTarget::Port {
                    node: NodeId::new(10),
                    port: 0,
                }),
                0,
            )
            .is_none(),
            "wire-side selections name no object",
        );
        assert!(chase_preview(&surface, None, 0).is_none());
    }

    /// The range grain previews (an id-less strand, the peach); a whole
    /// FIXTURE never does.
    ///
    /// The chase is the object language (D9, round 3): a fixture target
    /// breathes, so a chase painted on its sprites would be the canvas
    /// saying one thing while the wire says another — and on a multi-run
    /// fixture it painted two heads, which is what the gate rejected. That
    /// holds even for the scarf, the count-only strand with no object table:
    /// the matrix keys on the selection KIND, and no surface may special-case
    /// its way to a second answer.
    #[test]
    fn the_range_grain_previews_and_a_whole_fixture_never_does() {
        let mut surface = half_patched();
        let range = UiPatchTarget::Range {
            node: dome(),
            start: 30,
            count: Some(30),
        };
        assert_eq!(
            chase_preview(&surface, Some(&range), 0)
                .expect("preview")
                .colors
                .len(),
            30,
        );

        // A whole fixture WITH objects is a card, not an object (Q8).
        surface.fixtures[0].patch.cells.clear();
        assert!(
            chase_preview(&surface, Some(&UiPatchTarget::Fixture { node: dome() }), 0).is_none(),
            "the fixture card stops pretending the fixture is one object",
        );

        // …and the scarf's whole-fixture selection breathes like any other
        // fixture: no object chase here either.
        surface.fixtures[0].instances.clear();
        assert!(
            chase_preview(&surface, Some(&UiPatchTarget::Fixture { node: dome() }), 0).is_none(),
            "a fixture target speaks breath, whatever its object table holds",
        );
        // Its lamps still chase when they are named as a RANGE — the grain
        // an id-less strand actually patches at.
        let whole = chase_preview(
            &surface,
            Some(&UiPatchTarget::Range {
                node: dome(),
                start: 0,
                count: None,
            }),
            0,
        )
        .expect("preview");
        assert_eq!((whole.start, whole.colors.len()), (0, 60));
    }

    /// Story determinism, in ONE place: with no frames the phase is the
    /// frozen still; with frames it walks one sweep every
    /// [`PREVIEW_SWEEP_FRAMES`] and comes back round.
    #[test]
    fn the_phase_freezes_without_frames_and_sweeps_with_them() {
        assert_eq!(preview_phase(0), FROZEN_PREVIEW_PHASE);
        assert_eq!(preview_phase(PREVIEW_SWEEP_FRAMES), 0.0);
        assert_eq!(preview_phase(PREVIEW_SWEEP_FRAMES / 2), 0.5);
        assert_eq!(
            preview_phase(PREVIEW_SWEEP_FRAMES * 7 + 6),
            preview_phase(6),
            "the sweep wraps",
        );
        assert!((0.0..1.0).contains(&preview_phase(u64::MAX)));
    }

    /// The same surface previewed at two different frame counts paints two
    /// different pictures — the panel and the canvas move together because
    /// they read this one value, not because they each keep a clock.
    #[test]
    fn the_preview_advances_with_the_frame_clock() {
        let surface = half_patched();
        let still = chase_preview(&surface, Some(&sector("/sector/2")), 0).expect("preview");
        let later = chase_preview(&surface, Some(&sector("/sector/2")), 3).expect("preview");
        assert_ne!(still.colors, later.colors);
        assert_ne!(still.phase, later.phase);
    }
}

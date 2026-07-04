//! Eager stack operations on the persistent region.
//!
//! Every push/pop mutates the region immediately: a watchdog reset at any
//! instant must leave a readable stack. The torn-write rule is enforced
//! here — the frame record is fully written *before* the depth word makes
//! it visible, with a compiler fence between so the stores cannot be
//! reordered.

use core::sync::atomic::{Ordering, compiler_fence};

use crate::crash_record::CompactFrameName;
use crate::frame_kind::FrameKind;
use crate::frame_path::{FramePath, MAX_FRAME_DEPTH};
use crate::recovery_region::RecoveryRegion;

/// Push a frame. Returns the frame's name hash, or `None` when the stack
/// is at [`MAX_FRAME_DEPTH`].
pub(crate) fn push_frame(region: &mut RecoveryRegion, kind: FrameKind, name: &str) -> Option<u32> {
    let depth = region.depth() as usize;
    if depth >= MAX_FRAME_DEPTH {
        return None;
    }
    region.frames_mut()[depth].set(kind, name);
    let hash = region.frames()[depth].name_hash();
    // Record bytes must be globally visible before the depth word says the
    // slot is live; a WDT reset between the two stores just loses the push.
    compiler_fence(Ordering::SeqCst);
    region.set_depth((depth + 1) as u32);
    hash.into()
}

/// Pop the top frame. Returns `true` when the popped frame's hash matched
/// `expected_hash` (LIFO discipline held). The pop happens regardless —
/// a mismatch means guard discipline broke and the caller should assert.
pub(crate) fn pop_frame(region: &mut RecoveryRegion, expected_hash: u32) -> bool {
    let depth = region.depth() as usize;
    if depth == 0 || depth > MAX_FRAME_DEPTH {
        return false;
    }
    let matched = region.frames()[depth - 1].name_hash() == expected_hash;
    region.set_depth((depth - 1) as u32);
    matched
}

/// Snapshot the live stack as an identity path.
pub(crate) fn current_path(region: &RecoveryRegion) -> FramePath {
    let depth = (region.depth() as usize).min(MAX_FRAME_DEPTH);
    let mut path = FramePath::EMPTY;
    for frame in region.frames().iter().take(depth) {
        if let Some(kind) = frame.kind() {
            path.push(kind, frame.name_hash());
        }
    }
    path
}

/// Snapshot the live stack's display names (for crash records).
pub(crate) fn current_path_names(region: &RecoveryRegion) -> [CompactFrameName; MAX_FRAME_DEPTH] {
    let depth = (region.depth() as usize).min(MAX_FRAME_DEPTH);
    let mut names = [CompactFrameName::EMPTY; MAX_FRAME_DEPTH];
    for (i, frame) in region.frames().iter().take(depth).enumerate() {
        names[i].set(frame.kind_raw(), frame.name());
    }
    names
}

/// Drop all live frames (used once per boot after prior-run analysis).
pub(crate) fn clear_stack(region: &mut RecoveryRegion) {
    region.set_depth(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_record::fnv1a_32;
    use crate::reset_cause::ResetCause;

    fn name_hash(name: &str) -> u32 {
        fnv1a_32(name.as_bytes())
    }

    fn fresh_region() -> RecoveryRegion {
        let mut region = RecoveryRegion::ZEROED;
        region.reinit();
        region
    }

    #[test]
    fn push_pop_round_trip() {
        let mut region = fresh_region();
        let h1 = push_frame(&mut region, FrameKind::Boot, "boot").unwrap();
        let h2 = push_frame(&mut region, FrameKind::NodeRender, "nodes/a").unwrap();
        assert_eq!(region.depth(), 2);

        let path = current_path(&region);
        assert_eq!(path.len(), 2);
        assert_eq!(path.entry(1), Some((FrameKind::NodeRender as u8, h2)));

        assert!(pop_frame(&mut region, h2));
        assert!(pop_frame(&mut region, h1));
        assert_eq!(region.depth(), 0);
        assert!(
            !pop_frame(&mut region, h1),
            "pop on empty stack is a mismatch"
        );
    }

    #[test]
    fn push_saturates_at_max_depth() {
        let mut region = fresh_region();
        for i in 0..MAX_FRAME_DEPTH {
            assert!(
                push_frame(&mut region, FrameKind::NodeRender, "x").is_some(),
                "push {i}"
            );
        }
        assert!(push_frame(&mut region, FrameKind::NodeRender, "overflow").is_none());
        assert_eq!(region.depth() as usize, MAX_FRAME_DEPTH);
    }

    #[test]
    fn out_of_order_pop_reports_mismatch_but_still_pops() {
        let mut region = fresh_region();
        let h1 = push_frame(&mut region, FrameKind::Boot, "boot").unwrap();
        let _h2 = push_frame(&mut region, FrameKind::NodeRender, "nodes/a").unwrap();
        assert!(
            !pop_frame(&mut region, h1),
            "popped nodes/a while expecting boot"
        );
        assert_eq!(region.depth(), 1);
    }

    #[test]
    fn frame_payload_is_written_before_depth_advances() {
        // The torn-write invariant, observed at the API level: after a push,
        // the slot below the new depth is fully populated; before the push,
        // depth never exposes a half-written slot (we can only assert the
        // "after" half in safe code, plus that stale slots beyond depth are
        // never surfaced by current_path).
        let mut region = fresh_region();
        push_frame(&mut region, FrameKind::Boot, "boot");
        push_frame(&mut region, FrameKind::ProjectLoad, "p");
        pop_frame(&mut region, name_hash("p"));
        // Stale record for "p" still sits at slot 1, beyond depth:
        assert_eq!(region.frames()[1].name(), "p");
        // ...but identity snapshots never read past depth.
        assert_eq!(current_path(&region).len(), 1);
        assert_eq!(current_path_names(&region)[1].name(), "");
    }

    #[test]
    fn names_snapshot_matches_stack() {
        let mut region = fresh_region();
        push_frame(&mut region, FrameKind::Boot, "boot");
        push_frame(&mut region, FrameKind::ShaderCompile, "shaders/fire.glsl");
        let names = current_path_names(&region);
        assert_eq!(names[0].kind(), Some(FrameKind::Boot));
        assert_eq!(names[1].name(), "shaders/fire.g"); // truncated to 14 bytes
        assert_eq!(names[1].kind(), Some(FrameKind::ShaderCompile));
    }

    #[test]
    fn cleared_stack_is_empty_and_region_stays_valid() {
        let mut region = fresh_region();
        push_frame(&mut region, FrameKind::Boot, "boot");
        clear_stack(&mut region);
        assert_eq!(region.depth(), 0);
        // Stack ops never touch the CRC-covered areas.
        assert!(region.is_valid(ResetCause::SoftwareReset));
    }
}

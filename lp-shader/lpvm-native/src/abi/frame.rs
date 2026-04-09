//! Stack frame layout derived from [`super::FuncAbi`] and register allocation results.

use alloc::vec::Vec;

use crate::abi::{FuncAbi, PReg, PregSet};

/// Classifies a stack slot for debugging / tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Spill { index: u32 },
    Lpir { slot_id: u32, size: u32 },
}

/// Computed RV32 stack frame (sizes and offsets). Offsets are in **bytes**.
///
/// After prologue, `SP` points to the bottom of this frame (lowest address). Positive offsets
/// from `SP` move toward higher addresses (toward the caller’s stack).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    pub total_size: u32,
    pub save_ra: bool,
    pub save_fp: bool,
    /// SP-relative byte offset to saved RA, if saved.
    pub ra_offset_from_sp: Option<i32>,
    /// SP-relative byte offset to saved FP (`s0`), if saved.
    pub fp_offset_from_sp: Option<i32>,
    /// Callee-saved GPRs and their SP-relative save offsets (ascending with address).
    pub callee_save_offsets: Vec<(PReg, i32)>,
    pub spill_count: u32,
    /// Byte offset from `SP` to the first byte of spill slot 0.
    spill_base_from_sp: i32,
    /// LPIR semantic slots: `(slot_id, offset_from_sp)` to first byte.
    pub lpir_slot_offsets: Vec<(u32, i32)>,
}

impl FrameLayout {
    /// `used_callee_saved`: callee-saved GPRs assigned by regalloc (excluding roles such as sret
    /// preservation — the caller passes only what must be spilled to the stack for calls).
    pub fn compute(
        _abi: &FuncAbi,
        spill_count: u32,
        used_callee_saved: PregSet,
        lpir_slot_sizes: &[(u32, u32)],
        is_leaf: bool,
    ) -> Self {
        let save_ra = !is_leaf;
        let save_fp = true;

        let mut callee_list: Vec<PReg> = used_callee_saved.iter().collect();
        callee_list.sort_by_key(|p| (p.class as u8, p.hw));

        let spill_bytes = spill_count.saturating_mul(4);
        let lpir_bytes: u32 = lpir_slot_sizes.iter().map(|(_, s)| *s).sum();
        let body_bytes = spill_bytes.saturating_add(lpir_bytes);

        let link_bytes = (if save_ra { 4u32 } else { 0 })
            .saturating_add(if save_fp { 4 } else { 0 })
            .saturating_add(callee_list.len() as u32 * 4);

        let raw_total = body_bytes.saturating_add(link_bytes);
        let total_size = (raw_total.saturating_add(15)) & !15;

        let spill_base_from_sp = 0i32;

        let mut lpir_slot_offsets = Vec::with_capacity(lpir_slot_sizes.len());
        let mut pos = spill_bytes as i32;
        for (id, sz) in lpir_slot_sizes.iter().copied() {
            lpir_slot_offsets.push((id, pos));
            pos += sz as i32;
        }

        let mut cursor = total_size as i32;
        let mut ra_offset_from_sp = None;
        let mut fp_offset_from_sp = None;

        if save_ra {
            cursor -= 4;
            ra_offset_from_sp = Some(cursor);
        }
        if save_fp {
            cursor -= 4;
            fp_offset_from_sp = Some(cursor);
        }

        let mut callee_save_offsets = Vec::with_capacity(callee_list.len());
        for r in callee_list {
            cursor -= 4;
            callee_save_offsets.push((r, cursor));
        }
        callee_save_offsets.sort_by_key(|(_, o)| *o);

        Self {
            total_size,
            save_ra,
            save_fp,
            ra_offset_from_sp,
            fp_offset_from_sp,
            callee_save_offsets,
            spill_count,
            spill_base_from_sp,
            lpir_slot_offsets,
        }
    }

    /// SP-relative offset to spill slot `index` (slot 0 is lowest-address spill).
    pub fn spill_offset_from_sp(&self, index: u32) -> Option<i32> {
        if index < self.spill_count {
            Some(self.spill_base_from_sp + (index * 4) as i32)
        } else {
            None
        }
    }

    /// Same memory as [`Self::spill_offset_from_sp`], expressed so that
    /// `spill_offset_from_fp(k) = spill_offset_from_sp(k) + total_size` (frame high edge).
    pub fn spill_offset_from_fp(&self, index: u32) -> Option<i32> {
        self.spill_offset_from_sp(index)
            .map(|o| o + self.total_size as i32)
    }

    pub fn lpir_offset_from_sp(&self, slot_id: u32) -> Option<i32> {
        self.lpir_slot_offsets
            .iter()
            .find(|(id, _)| *id == slot_id)
            .map(|(_, o)| *o)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::abi::classify::entry_param_scalar_count;
    use crate::isa::rv32::abi as rv32;
    use lps_shared::{LpsFnSig, LpsType};

    fn abi_float() -> FuncAbi {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        rv32::func_abi_rv32(&sig, entry_param_scalar_count(&sig))
    }

    #[test]
    fn leaf_skips_ra_save_flag() {
        let abi = abi_float();
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &[], true);
        assert!(!frame.save_ra);
        assert!(frame.save_fp);
        assert!(frame.ra_offset_from_sp.is_none());
        assert!(frame.fp_offset_from_sp.is_some());
    }

    #[test]
    fn non_leaf_saves_ra() {
        let abi = abi_float();
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &[], false);
        assert!(frame.save_ra);
        assert!(frame.ra_offset_from_sp.is_some());
        assert!(frame.fp_offset_from_sp.is_some());
        assert!(frame.ra_offset_from_sp.unwrap() > frame.fp_offset_from_sp.unwrap());
    }

    #[test]
    fn callee_saved_get_offsets() {
        let abi = abi_float();
        let used = PregSet::singleton(rv32::S2).union(PregSet::singleton(rv32::S3));
        let frame = FrameLayout::compute(&abi, 0, used, &[], true);
        assert_eq!(frame.callee_save_offsets.len(), 2);
    }

    #[test]
    fn spill_offsets_step_by_four() {
        let abi = abi_float();
        let frame = FrameLayout::compute(&abi, 3, PregSet::EMPTY, &[], true);
        assert_eq!(frame.spill_count, 3);
        let o0 = frame.spill_offset_from_sp(0).unwrap();
        let o1 = frame.spill_offset_from_sp(1).unwrap();
        let o2 = frame.spill_offset_from_sp(2).unwrap();
        assert_eq!(o1 - o0, 4);
        assert_eq!(o2 - o1, 4);
        assert!(frame.spill_offset_from_sp(3).is_none());
    }

    #[test]
    fn total_size_aligned_16() {
        let abi = abi_float();
        for spill in [0u32, 3, 5] {
            let frame = FrameLayout::compute(&abi, spill, PregSet::EMPTY, &[], false);
            assert_eq!(frame.total_size % 16, 0);
        }
    }

    #[test]
    fn spill_fp_relation_matches_total_size() {
        let abi = abi_float();
        let frame = FrameLayout::compute(&abi, 2, PregSet::EMPTY, &[], true);
        let sp0 = frame.spill_offset_from_sp(0).unwrap();
        let fp0 = frame.spill_offset_from_fp(0).unwrap();
        assert!(fp0 > sp0);
        assert_eq!(fp0 - sp0, frame.total_size as i32);
    }

    #[test]
    fn lpir_slots_recorded() {
        let abi = abi_float();
        let lpir = [(0u32, 16u32), (1u32, 8u32)];
        let frame = FrameLayout::compute(&abi, 0, PregSet::EMPTY, &lpir, true);
        assert_eq!(frame.lpir_offset_from_sp(0), Some(0));
        assert_eq!(frame.lpir_offset_from_sp(1), Some(16));
    }
}

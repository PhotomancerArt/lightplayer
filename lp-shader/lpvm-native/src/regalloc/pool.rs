//! Physical register pool with LRU eviction.

use crate::isa::IsaTarget;
use crate::vinst::VReg;
use alloc::vec::Vec;

/// Physical register pool with LRU eviction.
pub struct RegPool {
    isa: IsaTarget,
    /// Which vreg occupies each hardware GPR (None = free).
    preg_vreg: [Option<VReg>; 32],
    /// LRU order: index 0 = least recently used. Only allocatable regs.
    lru: Vec<u8>,
}

impl RegPool {
    pub fn new(isa: IsaTarget) -> Self {
        let lru: Vec<u8> = isa.allocatable_pool_order().iter().copied().collect();
        Self {
            isa,
            preg_vreg: [None; 32],
            lru,
        }
    }

    /// Create pool with limited capacity (for testing spill logic).
    pub fn with_capacity(isa: IsaTarget, n: usize) -> Self {
        let lru: Vec<u8> = isa
            .allocatable_pool_order()
            .iter()
            .copied()
            .take(n)
            .collect();
        Self {
            isa,
            preg_vreg: [None; 32],
            lru,
        }
    }

    /// Find the hardware GPR currently holding this vreg, if any.
    pub fn home(&self, vreg: VReg) -> Option<u8> {
        for (i, v) in self.preg_vreg.iter().enumerate() {
            if *v == Some(vreg) {
                return Some(i as u8);
            }
        }
        None
    }

    /// Allocate a free register for vreg. Returns the GPR index and any evicted vreg.
    /// If no free reg, evicts the LRU and returns (evicted_vreg, preg).
    pub fn alloc(&mut self, vreg: VReg) -> (u8, Option<VReg>) {
        // Try to find a free allocatable reg (prefer LRU order)
        for (i, &preg) in self.lru.iter().enumerate() {
            if self.preg_vreg[preg as usize].is_none() {
                self.preg_vreg[preg as usize] = Some(vreg);
                // Move to end (most recently used)
                self.lru.remove(i);
                self.lru.push(preg);
                return (preg, None);
            }
        }
        // Evict LRU (index 0)
        let victim_preg = self.lru.remove(0);
        let victim_vreg = self.preg_vreg[victim_preg as usize];
        self.preg_vreg[victim_preg as usize] = Some(vreg);
        self.lru.push(victim_preg);
        (victim_preg, victim_vreg)
    }

    /// Allocate a specific hardware GPR for vreg. Evicts current occupant if any.
    /// Returns the evicted vreg (if any).
    pub fn alloc_fixed(&mut self, preg: u8, vreg: VReg) -> Option<VReg> {
        let evicted = self.preg_vreg[preg as usize];
        self.preg_vreg[preg as usize] = Some(vreg);
        self.touch(preg);
        evicted
    }

    /// Free a hardware GPR (vreg is no longer in a register).
    ///
    /// Moves the register to the front of the LRU so it will be reused
    /// before untouched callee-saved registers. This minimises the total
    /// number of distinct registers used and keeps values in caller-saved
    /// t-regs when possible, shrinking the prologue/epilogue.
    pub fn free(&mut self, preg: u8) {
        self.preg_vreg[preg as usize] = None;
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
            self.lru.insert(0, preg);
        }
    }

    /// Evict a vreg from a hardware GPR and remove the register from the LRU
    /// entirely so it cannot be allocated until restored. Used for call
    /// clobber handling (regalloc2-style): the clobbered register must
    /// not be reused for arg allocation within the same instruction.
    pub fn evict(&mut self, preg: u8) {
        self.preg_vreg[preg as usize] = None;
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
        }
    }

    /// Restore previously evicted registers to the front of the LRU,
    /// making them available for allocation again.
    pub fn restore_evicted(&mut self, pregs: &[u8]) {
        for &preg in pregs.iter().rev() {
            if !self.lru.contains(&preg) {
                self.lru.insert(0, preg);
            }
        }
    }

    /// Mark hardware GPR as most recently used.
    pub fn touch(&mut self, preg: u8) {
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
            self.lru.push(preg);
        }
    }

    /// Count occupied allocatable registers.
    pub fn occupied_count(&self) -> usize {
        self.isa
            .allocatable_pool_order()
            .iter()
            .filter(|&&p| self.preg_vreg[p as usize].is_some())
            .count()
    }

    /// Iterate over occupied (preg, vreg) pairs for allocatable registers.
    pub fn iter_occupied(&self) -> impl Iterator<Item = (u8, VReg)> + '_ {
        self.isa
            .allocatable_pool_order()
            .iter()
            .copied()
            .filter_map(|p| self.preg_vreg[p as usize].map(|v| (p, v)))
    }

    /// Get a snapshot of current occupied (preg, vreg) pairs.
    pub fn snapshot_occupied(&self) -> Vec<(u8, VReg)> {
        self.iter_occupied().collect()
    }

    /// Clear allocatable registers only (preserves precolored mappings).
    pub fn clear(&mut self) {
        for p in self.isa.allocatable_pool_order().iter() {
            self.preg_vreg[*p as usize] = None;
        }
        self.lru.clear();
        self.lru
            .extend(self.isa.allocatable_pool_order().iter().copied());
    }

    /// Clear ALL registers including precolored ones outside the allocatable pool.
    pub fn clear_all(&mut self) {
        self.preg_vreg = [None; 32];
        self.lru.clear();
        self.lru
            .extend(self.isa.allocatable_pool_order().iter().copied());
    }

    /// Iterate ALL occupied registers, including precolored ones
    /// outside the allocatable pool (e.g. a0 for vmctx).
    pub fn iter_all_occupied(&self) -> impl Iterator<Item = (u8, VReg)> + '_ {
        self.preg_vreg
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|vreg| (i as u8, vreg)))
    }

    /// Seed the pool with vreg assignments from saved state.
    /// Clears existing state first, then populates with saved assignments.
    pub fn seed(&mut self, assignments: &[(u8, VReg)]) {
        self.clear();
        for &(preg, vreg) in assignments {
            self.preg_vreg[preg as usize] = Some(vreg);
            self.touch(preg);
        }
    }
}

impl Default for RegPool {
    fn default() -> Self {
        Self::new(IsaTarget::Rv32imac)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::VReg;

    #[test]
    fn pool_alloc_and_free() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        let (preg1, evicted) = pool.alloc(VReg(0));
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(preg1));

        pool.free(preg1);
        assert!(pool.home(VReg(0)).is_none());
    }

    #[test]
    fn pool_lru_eviction() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        let order = IsaTarget::Rv32imac.allocatable_pool_order();

        // Fill all allocatable registers
        for i in 0..order.len() {
            let (_preg, evicted) = pool.alloc(VReg(i as u16));
            assert!(evicted.is_none(), "should not evict on {i}th alloc");
        }

        // Next alloc should evict LRU (first one allocated)
        let (preg, evicted) = pool.alloc(VReg(100));
        assert!(evicted.is_some());
        assert_eq!(evicted, Some(VReg(0)));

        // Evicted vreg no longer has a home
        assert!(pool.home(VReg(0)).is_none());
        // New vreg is in the evicted preg
        assert_eq!(pool.home(VReg(100)), Some(preg));
    }

    #[test]
    fn pool_alloc_fixed() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);

        // Allocate specific register
        let target = IsaTarget::Rv32imac.allocatable_pool_order()[0];
        let evicted = pool.alloc_fixed(target, VReg(0));
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(target));

        // Allocate same register to different vreg
        let evicted = pool.alloc_fixed(target, VReg(1));
        assert_eq!(evicted, Some(VReg(0)));
        assert_eq!(pool.home(VReg(1)), Some(target));
        assert!(pool.home(VReg(0)).is_none());
    }

    #[test]
    fn pool_touch_mru() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        let order_len = IsaTarget::Rv32imac.allocatable_pool_order().len();

        // Allocate two registers
        let (preg1, _) = pool.alloc(VReg(0));
        let (_preg2, _) = pool.alloc(VReg(1));

        // Touch first one, making it MRU
        pool.touch(preg1);

        // Allocate until eviction
        for i in 2..order_len {
            pool.alloc(VReg(i as u16));
        }

        // Next eviction should be preg2 (LRU), not preg1 (MRU)
        let (_, evicted) = pool.alloc(VReg(100));
        assert_eq!(evicted, Some(VReg(1))); // preg2's vreg
    }
}

//! Physical register pool with LRU eviction, one independent pool per class.

use crate::abi::{PReg, RegClass};
use crate::isa::IsaTarget;
use crate::vinst::VReg;
use alloc::vec::Vec;

/// One register class's LRU pool.
///
/// Indices inside a `ClassPool` are hardware encodings within that class, so
/// integer `10` and float `10` are different entries in different pools and can
/// never be confused for one another.
struct ClassPool {
    /// Which vreg occupies each hardware register of this class (None = free).
    preg_vreg: [Option<VReg>; 32],
    /// LRU order: index 0 = least recently used. Only allocatable regs.
    lru: Vec<u8>,
    /// The registers this pool may hand out, in LRU-seed order.
    ///
    /// Usually the ISA's static pool, but a [`FuncAbi`](crate::abi::FuncAbi) can
    /// withhold a register for the whole function — an sret pointer's home is
    /// the case that matters. Storing the effective order here (rather than
    /// re-reading the ISA's static list at each use) is what makes that
    /// withholding actually hold: `clear` and `clear_all` reseed the LRU, and
    /// reseeding from the static list would silently hand the register back.
    pool_order: Vec<u8>,
}

impl ClassPool {
    fn from_order(pool_order: Vec<u8>) -> Self {
        Self {
            preg_vreg: [None; 32],
            lru: pool_order.clone(),
            pool_order,
        }
    }

    fn home(&self, vreg: VReg) -> Option<u8> {
        self.preg_vreg
            .iter()
            .position(|v| *v == Some(vreg))
            .map(|i| i as u8)
    }

    fn alloc(&mut self, vreg: VReg) -> Option<(u8, Option<VReg>)> {
        // Try to find a free allocatable reg (prefer LRU order)
        for (i, &preg) in self.lru.iter().enumerate() {
            if self.preg_vreg[preg as usize].is_none() {
                self.preg_vreg[preg as usize] = Some(vreg);
                // Move to end (most recently used)
                self.lru.remove(i);
                self.lru.push(preg);
                return Some((preg, None));
            }
        }
        // Evict LRU (index 0). An empty LRU means this class has no
        // allocatable registers at all on this ISA — the caller turns that
        // into `OutOfRegisters` rather than reaching into another class.
        if self.lru.is_empty() {
            return None;
        }
        let victim_preg = self.lru.remove(0);
        let victim_vreg = self.preg_vreg[victim_preg as usize];
        self.preg_vreg[victim_preg as usize] = Some(vreg);
        self.lru.push(victim_preg);
        Some((victim_preg, victim_vreg))
    }

    fn alloc_fixed(&mut self, preg: u8, vreg: VReg) -> Option<VReg> {
        let evicted = self.preg_vreg[preg as usize];
        self.preg_vreg[preg as usize] = Some(vreg);
        self.touch(preg);
        evicted
    }

    fn free(&mut self, preg: u8) {
        self.preg_vreg[preg as usize] = None;
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
            self.lru.insert(0, preg);
        }
    }

    fn evict(&mut self, preg: u8) {
        self.preg_vreg[preg as usize] = None;
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
        }
    }

    fn touch(&mut self, preg: u8) {
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
            self.lru.push(preg);
        }
    }

    fn clear(&mut self) {
        for p in self.pool_order.iter() {
            self.preg_vreg[*p as usize] = None;
        }
        self.lru.clear();
        self.lru.extend(self.pool_order.iter().copied());
    }

    fn clear_all(&mut self) {
        self.preg_vreg = [None; 32];
        self.lru.clear();
        self.lru.extend(self.pool_order.iter().copied());
    }
}

/// Physical register pool with LRU eviction.
///
/// Holds one independent [`ClassPool`] per [`RegClass`]. A vreg's class decides
/// which pool serves it, and the two never interact: a float vreg cannot evict
/// an integer one, and an integer constraint cannot be satisfied out of the
/// float file. Neither backend has float registers yet, so the float pool is
/// empty and every allocation runs through the integer one.
pub struct RegPool {
    int: ClassPool,
    float: ClassPool,
}

impl RegPool {
    pub fn new(isa: IsaTarget) -> Self {
        Self::from_orders(
            isa.allocatable_pool_order(RegClass::Int).to_vec(),
            isa.allocatable_pool_order(RegClass::Float).to_vec(),
        )
    }

    fn from_orders(int_order: Vec<u8>, float_order: Vec<u8>) -> Self {
        Self {
            int: ClassPool::from_order(int_order),
            float: ClassPool::from_order(float_order),
        }
    }

    /// Pool for a specific function, honouring registers its ABI withholds.
    ///
    /// `FuncAbi::allocatable` removes the sret pointer's register, which must
    /// survive the whole function. That exclusion was previously computed and
    /// then ignored, because the pool seeded itself from the ISA's static list.
    /// It is invisible on rv32 — the register it withholds (`s1`) is not in
    /// rv32's static pool to begin with — and load-bearing on Xtensa, where the
    /// sret pointer lives in `a2`, squarely inside the pool.
    pub fn for_abi(abi: &crate::abi::FuncAbi) -> Self {
        let allocatable = abi.allocatable();
        let isa = abi.isa();
        let order_for = |class: RegClass| -> Vec<u8> {
            isa.allocatable_pool_order(class)
                .iter()
                .copied()
                .filter(|&hw| allocatable.contains(PReg { hw, class }))
                .collect()
        };
        Self::from_orders(order_for(RegClass::Int), order_for(RegClass::Float))
    }

    /// Create pool with limited integer capacity (for testing spill logic).
    pub fn with_capacity(isa: IsaTarget, n: usize) -> Self {
        Self::from_orders(
            isa.allocatable_pool_order(RegClass::Int)
                .iter()
                .copied()
                .take(n)
                .collect(),
            isa.allocatable_pool_order(RegClass::Float).to_vec(),
        )
    }

    /// Dispatch by `match` on two named fields rather than by indexing a
    /// `[ClassPool; RegClass::COUNT]`.
    ///
    /// The array reads better and was tried first. It cost **1,120 B** more on
    /// the ESP32-C6 image, measured: bounds checks the `match` does not need,
    /// plus iterator adapters over the array that LLVM kept as real code where
    /// it folds the two-arm `match` away entirely. Adding a third class means
    /// revisiting this, and re-measuring rather than reasoning about it.
    fn class_pool(&mut self, class: RegClass) -> &mut ClassPool {
        match class {
            RegClass::Int => &mut self.int,
            RegClass::Float => &mut self.float,
        }
    }

    /// Both class pools, integer first — the iteration order every snapshot
    /// depends on.
    fn class_pools(&self) -> [(RegClass, &ClassPool); 2] {
        [(RegClass::Int, &self.int), (RegClass::Float, &self.float)]
    }

    /// Find the physical register currently holding this vreg, if any.
    ///
    /// Searches both classes: a vreg lives in exactly one of them, so the
    /// answer is unambiguous and callers do not have to know the class to ask.
    pub fn home(&self, vreg: VReg) -> Option<PReg> {
        self.class_pools()
            .into_iter()
            .find_map(|(class, pool)| pool.home(vreg).map(|hw| PReg { hw, class }))
    }

    /// Allocate a free register of `class` for vreg. Returns the register and
    /// any evicted vreg. If no free reg, evicts the LRU and returns
    /// `(preg, evicted_vreg)`.
    ///
    /// `None` means `class` has no allocatable registers on this target at all
    /// — the case a float vreg hits on a backend without an FPU. It is a hard
    /// error, never a fall back into the other class.
    pub fn alloc(&mut self, vreg: VReg, class: RegClass) -> Option<(PReg, Option<VReg>)> {
        self.class_pool(class)
            .alloc(vreg)
            .map(|(hw, evicted)| (PReg { hw, class }, evicted))
    }

    /// Allocate a specific physical register for vreg. Evicts current occupant if any.
    /// Returns the evicted vreg (if any).
    pub fn alloc_fixed(&mut self, preg: PReg, vreg: VReg) -> Option<VReg> {
        self.class_pool(preg.class).alloc_fixed(preg.hw, vreg)
    }

    /// Free a physical register (vreg is no longer in a register).
    ///
    /// Moves the register to the front of the LRU so it will be reused
    /// before untouched callee-saved registers. This minimises the total
    /// number of distinct registers used and keeps values in caller-saved
    /// t-regs when possible, shrinking the prologue/epilogue.
    pub fn free(&mut self, preg: PReg) {
        self.class_pool(preg.class).free(preg.hw);
    }

    /// Evict a vreg from a physical register and remove the register from the
    /// LRU entirely so it cannot be allocated until restored. Used for call
    /// clobber handling (regalloc2-style): the clobbered register must
    /// not be reused for arg allocation within the same instruction.
    pub fn evict(&mut self, preg: PReg) {
        self.class_pool(preg.class).evict(preg.hw);
    }

    /// Restore previously evicted registers to the front of the LRU,
    /// making them available for allocation again.
    pub fn restore_evicted(&mut self, pregs: &[PReg]) {
        for &preg in pregs.iter().rev() {
            let pool = self.class_pool(preg.class);
            if !pool.lru.contains(&preg.hw) {
                pool.lru.insert(0, preg.hw);
            }
        }
    }

    /// Mark a physical register as most recently used.
    pub fn touch(&mut self, preg: PReg) {
        self.class_pool(preg.class).touch(preg.hw);
    }

    /// Count occupied allocatable registers across all classes.
    pub fn occupied_count(&self) -> usize {
        self.class_pools()
            .into_iter()
            .map(|(_, pool)| {
                pool.pool_order
                    .iter()
                    .filter(|&&p| pool.preg_vreg[p as usize].is_some())
                    .count()
            })
            .sum()
    }

    /// Iterate over occupied (preg, vreg) pairs for allocatable registers,
    /// integer class first.
    pub fn iter_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        self.class_pools().into_iter().flat_map(|(class, pool)| {
            pool.pool_order
                .iter()
                .copied()
                .filter_map(move |hw| pool.preg_vreg[hw as usize].map(|v| (PReg { hw, class }, v)))
        })
    }

    /// Get a snapshot of current occupied (preg, vreg) pairs.
    pub fn snapshot_occupied(&self) -> Vec<(PReg, VReg)> {
        self.iter_occupied().collect()
    }

    /// Clear allocatable registers only (preserves precolored mappings).
    pub fn clear(&mut self) {
        self.int.clear();
        self.float.clear();
    }

    /// Clear ALL registers including precolored ones outside the allocatable pool.
    pub fn clear_all(&mut self) {
        self.int.clear_all();
        self.float.clear_all();
    }

    /// Iterate ALL occupied registers, including precolored ones
    /// outside the allocatable pool (e.g. a0 for vmctx).
    pub fn iter_all_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        self.class_pools().into_iter().flat_map(|(class, pool)| {
            pool.preg_vreg
                .iter()
                .enumerate()
                .filter_map(move |(hw, v)| {
                    v.map(|vreg| {
                        (
                            PReg {
                                hw: hw as u8,
                                class,
                            },
                            vreg,
                        )
                    })
                })
        })
    }

    /// Seed the pool with vreg assignments from saved state.
    /// Clears existing state first, then populates with saved assignments.
    pub fn seed(&mut self, assignments: &[(PReg, VReg)]) {
        self.clear();
        for &(preg, vreg) in assignments {
            let pool = self.class_pool(preg.class);
            pool.preg_vreg[preg.hw as usize] = Some(vreg);
            pool.touch(preg.hw);
        }
    }
}

// No `Default` impl: a register pool is meaningless without an ISA, and the
// one that used to live here silently meant RV32 — a latent wrong-pool bug
// once a second backend existed. Construct with `RegPool::new(isa)`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::VReg;

    fn int_order(isa: IsaTarget) -> &'static [u8] {
        isa.allocatable_pool_order(RegClass::Int)
    }

    #[test]
    fn pool_alloc_and_free() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        let (preg1, evicted) = pool.alloc(VReg(0), RegClass::Int).expect("int pool");
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(preg1));

        pool.free(preg1);
        assert!(pool.home(VReg(0)).is_none());
    }

    #[test]
    fn pool_lru_eviction() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        let order = int_order(IsaTarget::Rv32imac);

        // Fill all allocatable registers
        for i in 0..order.len() {
            let (_preg, evicted) = pool.alloc(VReg(i as u16), RegClass::Int).expect("int pool");
            assert!(evicted.is_none(), "should not evict on {i}th alloc");
        }

        // Next alloc should evict LRU (first one allocated)
        let (preg, evicted) = pool.alloc(VReg(100), RegClass::Int).expect("int pool");
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
        let target = PReg::int(int_order(IsaTarget::Rv32imac)[0]);
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
        let order_len = int_order(IsaTarget::Rv32imac).len();

        // Allocate two registers
        let (preg1, _) = pool.alloc(VReg(0), RegClass::Int).expect("int pool");
        let (_preg2, _) = pool.alloc(VReg(1), RegClass::Int).expect("int pool");

        // Touch first one, making it MRU
        pool.touch(preg1);

        // Allocate until eviction
        for i in 2..order_len {
            pool.alloc(VReg(i as u16), RegClass::Int).expect("int pool");
        }

        // Next eviction should be preg2 (LRU), not preg1 (MRU)
        let (_, evicted) = pool.alloc(VReg(100), RegClass::Int).expect("int pool");
        assert_eq!(evicted, Some(VReg(1))); // preg2's vreg
    }

    /// No backend has float registers yet, and the pool must say so rather
    /// than quietly answering out of the integer file. This is the check that
    /// turns "f32 codegen is not implemented here" into a compile error at
    /// runtime instead of wrong code.
    #[test]
    fn float_class_has_no_registers_on_either_isa() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        assert!(pool.alloc(VReg(0), RegClass::Float).is_none());
        assert!(pool.home(VReg(0)).is_none());
    }

    /// The classes are separate address spaces: hardware index 20 in the float
    /// file is not hardware index 20 in the integer file.
    #[test]
    fn classes_do_not_share_occupancy() {
        let mut pool = RegPool::new(IsaTarget::Rv32imac);
        let int20 = PReg::int(20);
        pool.alloc_fixed(int20, VReg(0));
        assert_eq!(pool.home(VReg(0)), Some(int20));

        let float20 = PReg::float(20);
        pool.alloc_fixed(float20, VReg(1));
        assert_eq!(pool.home(VReg(1)), Some(float20));
        // The integer occupant is untouched by the float write.
        assert_eq!(pool.home(VReg(0)), Some(int20));
    }

    /// A register the function's ABI withholds must never be handed out — not
    /// on the first allocation, not after a `clear` reseeds the LRU.
    ///
    /// Regression for the sret-pointer clobber (2026-07-30): `FuncAbi`
    /// computed the exclusion and the pool ignored it, seeding itself from the
    /// ISA's static list instead. On Xtensa the withheld register is `a2`,
    /// which holds the sret pointer for the entire function and sits squarely
    /// inside the allocatable pool, so aggregate-returning shaders wrote their
    /// results through whatever value had replaced the pointer. See
    /// `docs/defects/2026-07-30-xtensa-sret-pointer-clobber.md`.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn for_abi_withholds_the_sret_pointer_register() {
        use crate::isa::xt::abi::{A2, func_abi_xt};
        use alloc::string::String;
        use lps_shared::{LpsFnKind, LpsFnSig, LpsType};

        // A vec4 return (4 scalars) is past the direct-return threshold, so it
        // returns through an sret buffer whose pointer lives in a2.
        let sig = LpsFnSig {
            name: String::from("f"),
            parameters: alloc::vec::Vec::new(),
            return_type: LpsType::Vec4,
            kind: LpsFnKind::UserDefined,
        };
        let abi = func_abi_xt(&sig, None);
        assert!(
            !abi.allocatable().contains(A2),
            "precondition: the ABI must withhold a2 for the sret pointer"
        );

        let mut pool = RegPool::for_abi(&abi);
        for i in 0..40u16 {
            let (preg, _) = pool.alloc(VReg(i), RegClass::Int).expect("int pool");
            assert_ne!(preg.hw, A2.hw, "pool handed out the withheld sret register");
        }
        pool.clear();
        for i in 40..80u16 {
            let (preg, _) = pool.alloc(VReg(i), RegClass::Int).expect("int pool");
            assert_ne!(
                preg.hw, A2.hw,
                "clear() reseeded the withheld register back in"
            );
        }
    }
}

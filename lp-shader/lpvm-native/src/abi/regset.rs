//! Physical registers and compact register sets for ABI2.

/// Integer vs float physical register class (RV32F uses float when implemented).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegClass {
    Int,
    Float,
}

impl RegClass {
    /// Every class, for the sweeps that must cover all of them — a call
    /// clobbering each class's caller-saved bank, say.
    pub const ALL: [RegClass; 2] = [RegClass::Int, RegClass::Float];
}

/// A physical register: hardware encoding plus class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PReg {
    pub hw: u8,
    pub class: RegClass,
}

impl PReg {
    pub const fn int(hw: u8) -> Self {
        Self {
            hw,
            class: RegClass::Int,
        }
    }

    pub const fn float(hw: u8) -> Self {
        Self {
            hw,
            class: RegClass::Float,
        }
    }

    fn bit_index(self) -> u32 {
        match self.class {
            RegClass::Int => self.hw as u32,
            RegClass::Float => 32 + self.hw as u32,
        }
    }
}

/// A [`PReg`] squeezed into one byte: class in bit 7, hardware index in bits 0–6.
///
/// Exists for exactly one caller — [`Alloc::Reg`](crate::regalloc::Alloc) — and
/// for exactly one reason. `Alloc` is stored in `AllocOutput::allocs`, a flat
/// table with one entry per operand of every instruction in the function; it is
/// the register allocator's largest single allocation and it is built on the
/// device, inside the JIT's heap. A `size_of::<Alloc>() == 2` assertion has
/// pinned that table's footprint since the ISA-decoupling refactor, and a
/// register class is one bit of information. Storing `PReg` inline would have
/// grown every entry by 50% to carry it, so the class rides in the spare high
/// bit of the byte that already held the hardware index.
///
/// Both classes index at most 32 registers, so seven bits is room to spare;
/// [`PackedPReg::new`] is `const` and total, and the round trip is exact.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PackedPReg(u8);

impl PackedPReg {
    /// Set when the register belongs to [`RegClass::Float`].
    const FLOAT_BIT: u8 = 0x80;

    pub const fn new(p: PReg) -> Self {
        debug_assert!(p.hw < 0x80);
        match p.class {
            RegClass::Int => Self(p.hw),
            RegClass::Float => Self(p.hw | Self::FLOAT_BIT),
        }
    }

    pub const fn int(hw: u8) -> Self {
        Self::new(PReg::int(hw))
    }

    pub const fn get(self) -> PReg {
        PReg {
            hw: self.hw(),
            class: self.class(),
        }
    }

    /// Hardware encoding, **ignoring class**. Only correct where the class is
    /// already established (an emitter that has just matched on it, say).
    pub const fn hw(self) -> u8 {
        self.0 & !Self::FLOAT_BIT
    }

    pub const fn class(self) -> RegClass {
        if self.0 & Self::FLOAT_BIT != 0 {
            RegClass::Float
        } else {
            RegClass::Int
        }
    }
}

impl core::fmt::Debug for PackedPReg {
    /// Renders as the [`PReg`] it encodes — the packing is a storage detail and
    /// should never show up in a panic message or a snapshot.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.get().fmt(f)
    }
}

/// Bitset of [`PReg`] values (32 int + 32 float lanes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PregSet(u64);

impl PregSet {
    pub const EMPTY: Self = Self(0);

    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn singleton(r: PReg) -> Self {
        Self(1u64 << r.bit_index())
    }

    pub fn contains(self, r: PReg) -> bool {
        (self.0 >> r.bit_index()) & 1 != 0
    }

    pub fn insert(&mut self, r: PReg) {
        self.0 |= 1u64 << r.bit_index();
    }

    pub fn remove(&mut self, r: PReg) {
        self.0 &= !(1u64 << r.bit_index());
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    pub fn iter(self) -> PregSetIter {
        PregSetIter(self.0)
    }
}

pub struct PregSetIter(u64);

impl Iterator for PregSetIter {
    type Item = PReg;

    fn next(&mut self) -> Option<PReg> {
        if self.0 == 0 {
            return None;
        }
        let idx = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        if idx < 32 {
            Some(PReg::int(idx as u8))
        } else {
            Some(PReg::float((idx - 32) as u8))
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn empty_contains_nothing() {
        let set = PregSet::EMPTY;
        assert!(!set.contains(PReg::int(0)));
        assert!(!set.contains(PReg::int(31)));
    }

    #[test]
    fn singleton() {
        let set = PregSet::singleton(PReg::int(5));
        assert!(set.contains(PReg::int(5)));
        assert!(!set.contains(PReg::int(4)));
        assert!(!set.contains(PReg::int(6)));
    }

    #[test]
    fn insert_remove() {
        let mut set = PregSet::EMPTY;
        set.insert(PReg::int(10));
        assert!(set.contains(PReg::int(10)));
        set.remove(PReg::int(10));
        assert!(!set.contains(PReg::int(10)));
    }

    #[test]
    fn union_intersection_difference() {
        let a = PregSet::singleton(PReg::int(1)).union(PregSet::singleton(PReg::int(2)));
        let b = PregSet::singleton(PReg::int(2)).union(PregSet::singleton(PReg::int(3)));
        assert_eq!(a.intersection(b).count(), 1);
        assert!(a.intersection(b).contains(PReg::int(2)));
        let d = a.difference(b);
        assert!(d.contains(PReg::int(1)));
        assert!(!d.contains(PReg::int(2)));
    }

    #[test]
    fn iter_yields_all() {
        let set = PregSet::singleton(PReg::int(1))
            .union(PregSet::singleton(PReg::int(5)))
            .union(PregSet::singleton(PReg::int(10)));
        let mut v: Vec<_> = set.iter().collect();
        v.sort_by_key(|p| p.hw);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn int_and_float_distinct() {
        let i = PReg::int(10);
        let f = PReg::float(10);
        let set = PregSet::singleton(i);
        assert!(set.contains(i));
        assert!(!set.contains(f));
    }

    #[test]
    fn packed_preg_round_trips_both_classes() {
        for hw in 0..32u8 {
            for p in [PReg::int(hw), PReg::float(hw)] {
                let packed = PackedPReg::new(p);
                assert_eq!(packed.get(), p);
                assert_eq!(packed.hw(), hw);
                assert_eq!(packed.class(), p.class);
            }
        }
    }

    /// The same hardware index in the two classes must not collide — that
    /// collision is the whole failure mode the class exists to prevent.
    #[test]
    fn packed_preg_separates_the_classes() {
        assert_ne!(PackedPReg::int(10), PackedPReg::new(PReg::float(10)));
    }

    /// The packing exists to keep `Alloc` at two bytes; if `PackedPReg` ever
    /// stops being one byte, that pin is already lost here.
    #[test]
    fn packed_preg_is_one_byte() {
        assert_eq!(core::mem::size_of::<PackedPReg>(), 1);
    }
}

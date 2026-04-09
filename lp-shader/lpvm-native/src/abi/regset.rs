//! Physical registers and compact register sets for ABI2.

/// Integer vs float physical register class (RV32F uses float when implemented).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegClass {
    Int,
    Float,
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
}

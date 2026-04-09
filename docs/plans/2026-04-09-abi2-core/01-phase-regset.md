# Phase 1: Register Set Abstraction

## Scope

Implement the core register abstractions in `abi2/regset.rs`:
- `RegClass` enum (Int, Float)
- `PReg` struct (hardware index + class)
- `PregSet` bitmask for efficient set operations

## Code Organization

- Entry point types at top of file
- Set operations (union, intersection, etc.) follow
- Iterator implementation at bottom
- Tests inline in `mod tests`

## Implementation Details

### RegClass Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegClass {
    Int,
    Float,
}
```

### PReg Struct

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PReg {
    pub hw: u8,           // 0-31 for RV32
    pub class: RegClass,
}

impl PReg {
    pub fn new_int(hw: u8) -> Self {
        assert!(hw < 32);
        Self { hw, class: RegClass::Int }
    }
    
    pub fn new_float(hw: u8) -> Self {
        assert!(hw < 32);
        Self { hw, class: RegClass::Float }
    }
}
```

### PregSet Bitmask

For RV32 we need 32 bits for integer registers, 32 bits for float registers = 64 bits total. Use `u64`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PregSet(u64);

impl PregSet {
    pub const EMPTY: Self = Self(0);
    
    pub fn singleton(r: PReg) -> Self {
        Self(1u64 << Self::index(r))
    }
    
    fn index(r: PReg) -> u32 {
        match r.class {
            RegClass::Int => r.hw as u32,
            RegClass::Float => 32 + r.hw as u32,
        }
    }

    pub fn contains(&self, r: PReg) -> bool {
        (self.0 >> Self::index(r)) & 1 != 0
    }
    
    pub fn insert(&mut self, r: PReg) {
        self.0 |= 1u64 << Self::index(r);
    }
    
    pub fn remove(&mut self, r: PReg) {
        self.0 &= !(1u64 << Self::index(r));
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
    
    pub fn count(&self) -> u32 {
        self.0.count_ones()
    }
    
    pub fn iter(&self) -> impl Iterator<Item = PReg> {
        PregSetIter(self.0)
    }
}

struct PregSetIter(u64);

impl Iterator for PregSetIter {
    type Item = PReg;
    fn next(&mut self) -> Option<PReg> {
        if self.0 == 0 {
            None
        } else {
            let idx = self.0.trailing_zeros();
            self.0 &= self.0 - 1;  // clear lowest bit
            let class = if idx < 32 { RegClass::Int } else { RegClass::Float };
            let hw = (idx % 32) as u8;
            Some(PReg { hw, class })
        }
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_set_contains_nothing() {
        let set = PregSet::EMPTY;
        assert!(!set.contains(PReg::new_int(0)));
        assert!(!set.contains(PReg::new_int(31)));
    }

    #[test]
    fn singleton_contains_only_one() {
        let set = PregSet::singleton(PReg::new_int(5));
        assert!(set.contains(PReg::new_int(5)));
        assert!(!set.contains(PReg::new_int(4)));
        assert!(!set.contains(PReg::new_int(6)));
    }

    #[test]
    fn insert_and_remove() {
        let mut set = PregSet::EMPTY;
        set.insert(PReg::new_int(10));
        assert!(set.contains(PReg::new_int(10)));
        set.remove(PReg::new_int(10));
        assert!(!set.contains(PReg::new_int(10)));
    }

    #[test]
    fn union_combines_sets() {
        let a = PregSet::singleton(PReg::new_int(1));
        let b = PregSet::singleton(PReg::new_int(2));
        let u = a.union(b);
        assert!(u.contains(PReg::new_int(1)));
        assert!(u.contains(PReg::new_int(2)));
        assert_eq!(u.count(), 2);
    }

    #[test]
    fn intersection_finds_common() {
        let a = PregSet::singleton(PReg::new_int(1)).union(PregSet::singleton(PReg::new_int(2)));
        let b = PregSet::singleton(PReg::new_int(2)).union(PregSet::singleton(PReg::new_int(3)));
        let i = a.intersection(b);
        assert!(i.contains(PReg::new_int(2)));
        assert!(!i.contains(PReg::new_int(1)));
        assert!(!i.contains(PReg::new_int(3)));
    }

    #[test]
    fn difference_removes_elements() {
        let a = PregSet::singleton(PReg::new_int(1)).union(PregSet::singleton(PReg::new_int(2)));
        let b = PregSet::singleton(PReg::new_int(2));
        let d = a.difference(b);
        assert!(d.contains(PReg::new_int(1)));
        assert!(!d.contains(PReg::new_int(2)));
    }

    #[test]
    fn iter_yields_all_elements() {
        let set = PregSet::singleton(PReg::new_int(1))
            .union(PregSet::singleton(PReg::new_int(5)))
            .union(PregSet::singleton(PReg::new_int(10)));
        let collected: Vec<_> = set.iter().collect();
        assert_eq!(collected.len(), 3);
        assert!(collected.contains(&PReg::new_int(1)));
        assert!(collected.contains(&PReg::new_int(5)));
        assert!(collected.contains(&PReg::new_int(10)));
    }

    #[test]
    fn int_and_float_are_distinct() {
        let int_a0 = PReg::new_int(10);
        let float_a0 = PReg::new_float(10);
        let set = PregSet::singleton(int_a0);
        assert!(set.contains(int_a0));
        assert!(!set.contains(float_a0));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native abi2::regset
```

All tests should pass. No other code changes yet.

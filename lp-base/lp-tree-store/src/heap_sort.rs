//! The store's one sort. core's sorts are kilobytes of RV32 code per
//! element type; a heap sort is a few hundred bytes and O(n log n), which
//! the mount-time index (hundreds to thousands of entries) needs. Its body
//! is shared by every element type (type-erased behind a `dyn FnMut`), so
//! a new sorted type costs only its compare and swap.

/// Sort `v` so that `less(a, b)` holds for no later `a` before earlier `b`
/// (not stable). A thin shell per element type over one shared,
/// non-generic heap sort that reaches the slice through `cmp_or_swap`.
pub fn heap_sort_by<T>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    let n = v.len();
    heap_sort_core(n, &mut |i, j, swap| {
        if swap {
            v.swap(i, j);
            false
        } else {
            less(&v[i], &v[j])
        }
    });
}

/// Strings by their bytes (the store's listings and the `LpFs` adapter's:
/// one copy for both).
pub fn sort_strings(v: &mut [alloc::string::String]) {
    heap_sort_by(v, |a, b| a.as_bytes() < b.as_bytes());
}

/// The sort over positions: `op(i, j, false)` = is `i` less than `j`;
/// `op(i, j, true)` swaps them.
#[inline(never)]
fn heap_sort_core(n: usize, op: &mut dyn FnMut(usize, usize, bool) -> bool) {
    for start in (0..n / 2).rev() {
        sift(op, start, n);
    }
    for end in (1..n).rev() {
        op(0, end, true);
        sift(op, 0, end);
    }
}

fn sift(op: &mut dyn FnMut(usize, usize, bool) -> bool, mut root: usize, end: usize) {
    loop {
        let mut child = 2 * root + 1;
        if child >= end {
            return;
        }
        if child + 1 < end && op(child, child + 1, false) {
            child += 1;
        }
        if !op(root, child, false) {
            return;
        }
        op(root, child, true);
        root = child;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn sorts() {
        let mut v = [5, 1, 4, 1, 3];
        heap_sort_by(&mut v, |a, b| a < b);
        assert_eq!(v, [1, 1, 3, 4, 5]);
        let mut e: [u8; 0] = [];
        heap_sort_by(&mut e, |a, b| a < b);
        let mut big: Vec<u32> = (0..1000u32)
            .map(|i| i.wrapping_mul(2_654_435_761))
            .collect();
        heap_sort_by(&mut big, |a, b| a < b);
        assert!(big.windows(2).all(|w| w[0] <= w[1]));
    }
}

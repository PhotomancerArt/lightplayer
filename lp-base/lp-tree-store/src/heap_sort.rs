//! The store's one sort. core's sorts are kilobytes of RV32 code per
//! element type; a heap sort is a few hundred bytes and O(n log n), which
//! the mount-time index (hundreds to thousands of entries) needs.

/// Sort `v` so that `less(a, b)` holds for no later `a` before earlier `b`
/// (not stable).
pub fn heap_sort_by<T>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    let n = v.len();
    let mut sift = |v: &mut [T], mut root: usize, end: usize| {
        loop {
            let mut child = 2 * root + 1;
            if child >= end {
                return;
            }
            if child + 1 < end && less(&v[child], &v[child + 1]) {
                child += 1;
            }
            if !less(&v[root], &v[child]) {
                return;
            }
            v.swap(root, child);
            root = child;
        }
    };
    for start in (0..n / 2).rev() {
        sift(v, start, n);
    }
    for end in (1..n).rev() {
        v.swap(0, end);
        sift(v, 0, end);
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

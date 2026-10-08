//! One tiny sort for the store's short lists (sector headers, roots, dir
//! entries, a victim's records, record sizes). core's sorts are kilobytes of
//! RV32 code per element type; this is a few dozen bytes each. Insertion
//! sort: quadratic, fine for the hundreds-of-items lists it sees.

/// Sort `v` so that `less(a, b)` holds for no later `a` before earlier `b`.
pub fn sort_small_by<T>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    for i in 1..v.len() {
        let mut j = i;
        while j > 0 && less(&v[j], &v[j - 1]) {
            v.swap(j, j - 1);
            j -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts() {
        let mut v = [5, 1, 4, 1, 3];
        sort_small_by(&mut v, |a, b| a < b);
        assert_eq!(v, [1, 1, 3, 4, 5]);
        let mut e: [u8; 0] = [];
        sort_small_by(&mut e, |a, b| a < b);
    }
}

//! Sorted JIT entry offsets → per-function sizes (`next - cur`, last uses `buffer_len`).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Stable sort of `name → byte offset` by offset, then name.
pub(crate) fn sort_by_offset<'a>(map: &'a BTreeMap<String, usize>) -> Vec<(&'a str, u32)> {
    let mut sorted: Vec<(&'a str, u32)> = map
        .iter()
        .map(|(name, off)| (name.as_str(), *off as u32))
        .collect();
    sorted.sort_by(|(na, oa), (nb, ob)| oa.cmp(ob).then_with(|| na.cmp(nb)));
    sorted
}

/// `sorted_offsets` must be non-decreasing. For each index `i`, size is
/// `sorted_offsets[i+1] - sorted_offsets[i]`, or `buffer_len - sorted_offsets[i]` for the last.
pub(crate) fn derive_sizes(sorted_offsets: &[u32], buffer_len: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(sorted_offsets.len());
    for (i, &off) in sorted_offsets.iter().enumerate() {
        let next = sorted_offsets.get(i + 1).copied().unwrap_or(buffer_len);
        out.push(next.saturating_sub(off));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn sizes_are_offset_deltas() {
        let sorted = [0u32, 0x40, 0x60];
        let sizes = derive_sizes(&sorted, 0xa0);
        assert_eq!(sizes, alloc::vec![0x40u32, 0x20, 0x40]);
    }

    #[test]
    fn sizes_from_btreemap_offsets() {
        let mut offsets = BTreeMap::new();
        offsets.insert("alpha".to_string(), 0);
        offsets.insert("beta".to_string(), 0x40);
        offsets.insert("gamma".to_string(), 0x60);
        let sorted = sort_by_offset(&offsets);
        assert_eq!(
            sorted,
            alloc::vec![("alpha", 0), ("beta", 0x40), ("gamma", 0x60)]
        );
        let offs: Vec<u32> = sorted.iter().map(|(_, o)| *o).collect();
        let sizes = derive_sizes(&offs, 0xa0);
        assert_eq!(sizes, alloc::vec![0x40u32, 0x20, 0x40]);
    }
}

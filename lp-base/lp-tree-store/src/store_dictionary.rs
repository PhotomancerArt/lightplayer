//! Training the store-local deflate dictionary at push time.
//!
//! The prototype sampler: every 24-byte window at a 4-byte stride of the
//! pushed content is counted (FNV-1a hash); windows seen at least twice are
//! ranked by count, their first occurrences are marked, overlapping or
//! adjacent marks in one sample merge into spans, spans are deduplicated by
//! content, and the best-scoring spans are concatenated with the best last
//! (closest to the data, so deflate's distances are short), trimmed from the
//! front to `dict_size`. Simple and deterministic; not zstd's COVER.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

const WINDOW: usize = 24;
const STRIDE: usize = 4;

/// A dictionary of at most `dict_size` bytes from `samples`, or empty when
/// nothing repeats.
pub fn train_dictionary(samples: &[&[u8]], dict_size: usize) -> Vec<u8> {
    // hash → (count, first sample, first offset)
    let mut seen: BTreeMap<u64, (u32, usize, usize)> = BTreeMap::new();
    for (si, s) in samples.iter().enumerate() {
        let mut pos = 0;
        while pos + WINDOW <= s.len() {
            let h = fnv1a(&s[pos..pos + WINDOW]);
            seen.entry(h)
                .and_modify(|e| e.0 += 1)
                .or_insert((1, si, pos));
            pos += STRIDE;
        }
    }
    let mut ranked: Vec<(u32, usize, usize)> = seen.into_values().filter(|e| e.0 >= 2).collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    // Take windows, best first, until their raw length covers the budget.
    let mut marks: Vec<(usize, usize, u32)> = Vec::new(); // (sample, offset, count)
    let mut budget = 0usize;
    for (count, si, off) in ranked {
        if budget >= dict_size * 2 {
            break;
        }
        marks.push((si, off, count));
        budget += WINDOW;
    }
    marks.sort_unstable();
    // Merge overlapping/adjacent windows in one sample into spans.
    let mut spans: Vec<(u64, usize, usize, usize)> = Vec::new(); // (score, sample, start, end)
    for (si, off, count) in marks {
        let end = off + WINDOW;
        match spans.last_mut() {
            Some(sp) if sp.1 == si && off <= sp.3 => {
                sp.3 = sp.3.max(end);
                sp.0 += u64::from(count);
            }
            _ => spans.push((u64::from(count), si, off, end)),
        }
    }
    // Best spans last; dedupe equal spans.
    spans.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut uniq = BTreeSet::new();
    let mut picked: Vec<&[u8]> = Vec::new();
    let mut total = 0usize;
    for &(_, si, start, end) in spans.iter().rev() {
        let bytes = &samples[si][start..end];
        if !uniq.insert(fnv1a(bytes)) {
            continue;
        }
        picked.push(bytes);
        total += bytes.len();
        if total >= dict_size {
            break;
        }
    }
    let mut dict = Vec::with_capacity(total);
    for b in picked.iter().rev() {
        dict.extend_from_slice(b);
    }
    if dict.len() > dict_size {
        dict.drain(..dict.len() - dict_size);
    }
    dict
}

fn fnv1a(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repeated_text_within_budget() {
        let a = b"{\"kind\": \"shader\", \"params\": {\"speed\": 1.0, \"phase\": 0.5}}".repeat(4);
        let b = b"unrelated prefix {\"kind\": \"shader\", \"params\": {\"speed\": 3.0}}".to_vec();
        let d = train_dictionary(&[&a, &b], 64);
        assert!(!d.is_empty() && d.len() <= 64);
        assert!(train_dictionary(&[b"no repeats here"], 64).is_empty());
    }
}

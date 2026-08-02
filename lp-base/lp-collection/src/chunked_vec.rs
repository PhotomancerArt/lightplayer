//! Chunked vector for allocation-sensitive contexts (e.g. embedded).
//!
//! Allocates in small chunks instead of one large contiguous block to reduce
//! OOM risk from heap fragmentation when compiling on constrained heaps.
//! Shared by regalloc2 (fastalloc) and cranelift-codegen (VCode).

use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::ops::{Index, IndexMut};
use core::ptr;

/// Target upper bound on a single inner allocation, in **bytes**.
///
/// This used to be a flat 64 *elements*, which is not a bound on anything the
/// heap cares about. It was calibrated against `LpirOp` (20 B on a 32-bit
/// target → a 1,280 B chunk, comfortably inside the "~2–4 KB" the comment
/// claimed). `lps-glsl` later reused `ChunkedVec` for `HirExpr`, which is
/// **96 B** on that same target, and the bound silently became 6,144 B per
/// chunk — larger than the whole of the classic ESP32's remaining heap at the
/// point it OOMs. A collection whose entire purpose is to avoid large
/// contiguous allocations was making the largest one in the compiler.
///
/// 1 KiB is chosen to sit under the smallest hole a working classic reliably
/// has. `HirExpr` gets 10 elements per chunk (960 B), `LpirOp` 51 (1,020 B).
///
/// See `docs/defects/2026-08-02-classic-oom-retry-succeeds.md`.
const CHUNK_BYTES: usize = 1024;

/// A vector backed by multiple smaller allocations.
///
/// All chunks except the last have exactly [`Self::CHUNK_SIZE`] elements. The
/// last chunk is a Vec that grows naturally up to that, then a new chunk is
/// started. This limits peak allocation while avoiding over-allocation for
/// small collections.
#[derive(Clone, Debug)]
pub struct ChunkedVec<T> {
    chunks: Vec<Vec<T>>,
    len: usize,
}

impl<T> ChunkedVec<T> {
    /// Elements per chunk, derived from [`CHUNK_BYTES`] so the bound is on the
    /// allocation the heap sees rather than on a count that means nothing to it.
    ///
    /// Always at least 1, so an element larger than [`CHUNK_BYTES`] still gets a
    /// chunk of its own rather than a divide-by-zero. Zero-sized types never
    /// allocate, so their chunk size is arbitrary.
    pub(crate) const CHUNK_SIZE: usize = {
        let elem = core::mem::size_of::<T>();
        if elem == 0 {
            64
        } else if CHUNK_BYTES / elem == 0 {
            1
        } else {
            CHUNK_BYTES / elem
        }
    };

    /// Create an empty ChunkedVec.
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }

    /// Create with given length, filling with `default`.
    /// Used by fastalloc for pre-allocated VReg state.
    pub fn with_capacity_and_default(len: usize, default: T) -> Self
    where
        T: Clone,
    {
        let mut chunks = Vec::new();
        let mut remaining = len;
        while remaining > 0 {
            let chunk_len = remaining.min(Self::CHUNK_SIZE);
            chunks.push(vec![default.clone(); chunk_len]);
            remaining -= chunk_len;
        }
        Self { chunks, len }
    }

    /// Create a ChunkedVec with capacity for at least `cap` elements.
    /// The last chunk starts empty and grows normally up to CHUNK_SIZE.
    pub fn with_capacity(cap: usize) -> Self {
        let n_chunks = cap.div_ceil(Self::CHUNK_SIZE);
        let mut chunks = Vec::with_capacity(n_chunks.max(1));
        if cap > 0 {
            chunks.push(Vec::new());
        }
        Self { chunks, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Asserts all inner Vecs have len and capacity ≤ CHUNK_SIZE.
    #[cfg(test)]
    fn assert_inner_vecs_within_limit(&self) {
        for (i, chunk) in self.chunks.iter().enumerate() {
            assert!(
                chunk.len() <= Self::CHUNK_SIZE,
                "chunk {i} len {} > CHUNK_SIZE",
                chunk.len()
            );
            assert!(
                chunk.capacity() <= Self::CHUNK_SIZE,
                "chunk {i} capacity {} > CHUNK_SIZE",
                chunk.capacity()
            );
        }
    }

    #[inline]
    fn chunk_and_offset(&self, i: usize) -> (usize, usize) {
        if self.chunks.len() == 1 {
            (0, i)
        } else if i < (self.chunks.len() - 1) * Self::CHUNK_SIZE {
            (i / Self::CHUNK_SIZE, i % Self::CHUNK_SIZE)
        } else {
            let full_chunk_elems = (self.chunks.len() - 1) * Self::CHUNK_SIZE;
            (self.chunks.len() - 1, i - full_chunk_elems)
        }
    }

    pub fn push(&mut self, value: T) {
        let need_new_chunk = self.chunks.is_empty()
            || self
                .chunks
                .last()
                .map(|c| c.len() >= Self::CHUNK_SIZE)
                .unwrap_or(false);
        if need_new_chunk {
            self.chunks.push(Vec::new());
        }
        let last = self.chunks.len() - 1;
        self.chunks[last].push(value);
        self.len += 1;
    }

    /// Resize to `new_len`, filling new slots with `value` if growing.
    pub fn resize(&mut self, new_len: usize, value: T)
    where
        T: Clone,
    {
        if new_len <= self.len {
            self.len = new_len;
            let keep_chunks = new_len.div_ceil(Self::CHUNK_SIZE);
            if keep_chunks == 0 {
                self.chunks.clear();
                return;
            }
            self.chunks.truncate(keep_chunks);
            let last_offset = new_len % Self::CHUNK_SIZE;
            let last_len = if last_offset == 0 && new_len > 0 {
                Self::CHUNK_SIZE
            } else {
                last_offset
            };
            self.chunks[keep_chunks - 1].truncate(last_len);
            return;
        }
        while self.len < new_len {
            self.push(value.clone());
        }
    }

    /// Removes and returns the element at `index`, replacing it with the last element.
    /// Returns `None` if index is out of bounds.
    pub fn swap_remove(&mut self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        let last = self.len - 1;
        if index != last {
            let a = self.get_mut(index).expect("valid index") as *mut T;
            let b = self.get_mut(last).expect("valid index") as *mut T;
            unsafe { ptr::swap(a, b) };
        }
        let (ci, o) = self.chunk_and_offset(last);
        let chunk = self.chunks.get_mut(ci)?;
        let val = chunk.swap_remove(o);
        self.len -= 1;
        if chunk.is_empty() {
            self.chunks.pop();
        }
        Some(val)
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len {
            return None;
        }
        let (ci, o) = self.chunk_and_offset(i);
        self.chunks.get(ci).and_then(|c| c.get(o))
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i >= self.len {
            return None;
        }
        let (ci, o) = self.chunk_and_offset(i);
        self.chunks.get_mut(ci).and_then(|c| c.get_mut(o))
    }

    pub fn iter(&self) -> Iter<'_, T> {
        let mut chunks = self.chunks.iter();
        let current = chunks.next().map(|c| c.iter());
        Iter { chunks, current }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.chunks.iter_mut().flat_map(|c| c.iter_mut())
    }

    /// Reverse the sequence in place. O(n) time, no allocation.
    /// Uses element swap to preserve the chunk layout required by get/index.
    pub fn reverse(&mut self) {
        let len = self.len;
        for i in 0..len / 2 {
            let j = len - 1 - i;
            let a = self.get_mut(i).expect("valid index") as *mut T;
            let b = self.get_mut(j).expect("valid index") as *mut T;
            unsafe { ptr::swap(a, b) };
        }
    }

    /// Binary search with a comparator. Returns `Ok(idx)` if found, `Err(idx)` for insertion point.
    pub fn binary_search_by<F>(&self, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(&T) -> Ordering,
    {
        let mut size = self.len;
        let mut left = 0;
        let mut right = size;
        while left < right {
            let mid = left + size / 2;
            let cmp = f(self.get(mid).expect("mid in bounds"));
            match cmp {
                Ordering::Less => left = mid + 1,
                Ordering::Greater => right = mid,
                Ordering::Equal => return Ok(mid),
            }
            size = right - left;
        }
        Err(left)
    }

    /// Iterator over elements from `start` index to the end.
    pub fn iter_from(&self, start: usize) -> IterFrom<'_, T> {
        IterFrom {
            vec: self,
            index: start,
        }
    }

    /// Sort by key. Uses temporary allocation (size = len); for small collections this is acceptable.
    pub fn sort_by_key<K, F>(&mut self, f: F)
    where
        T: Clone,
        F: FnMut(&T) -> K,
        K: Ord,
    {
        let mut elems: Vec<T> = self.iter().cloned().collect();
        elems.sort_by_key(f);
        self.chunks.clear();
        self.len = 0;
        for item in elems {
            self.push(item);
        }
    }
}

/// Iterator over `ChunkedVec` elements.
pub struct Iter<'a, T> {
    chunks: core::slice::Iter<'a, Vec<T>>,
    current: Option<core::slice::Iter<'a, T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut it) = self.current
                && let Some(x) = it.next()
            {
                return Some(x);
            }
            self.current = self.chunks.next().map(|c| c.iter());
            self.current.as_ref()?;
        }
    }
}

impl<'a, T> IntoIterator for &'a ChunkedVec<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over `ChunkedVec` elements from a given index.
pub struct IterFrom<'a, T> {
    vec: &'a ChunkedVec<T>,
    index: usize,
}

impl<'a, T> Iterator for IterFrom<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.vec.len() {
            return None;
        }
        let item = self.vec.get(self.index);
        self.index += 1;
        item
    }
}

impl<T> Default for ChunkedVec<T> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl<T> Index<usize> for ChunkedVec<T> {
    type Output = T;

    #[inline]
    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("ChunkedVec index out of bounds")
    }
}

impl<T> IndexMut<usize> for ChunkedVec<T> {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        self.get_mut(i).expect("ChunkedVec index out of bounds")
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for ChunkedVec<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for item in self.iter() {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for ChunkedVec<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Vec::<T>::deserialize(deserializer)?;
        let mut chunked = ChunkedVec::new();
        for item in v {
            chunked.push(item);
        }
        Ok(chunked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunked_vec_new() -> ChunkedVec<i32> {
        ChunkedVec::new()
    }

    /// Elements per chunk for the type these tests use.
    ///
    /// Every size below is expressed in terms of it rather than hardcoded. The
    /// original tests used literals (130, 200, 600) chosen against a 64-element
    /// chunk; once the bound became per-type those literals all fit inside a
    /// single `i32` chunk, so the multi-chunk tests would have kept passing
    /// while testing nothing. Boundary crossings have to be structural.
    const CHUNK: usize = ChunkedVec::<i32>::CHUNK_SIZE;

    #[test]
    fn chunk_size_is_bounded_in_bytes_not_elements() {
        // The regression this collection exists to prevent: an element type big
        // enough that a chunk stops being a small allocation. `HirExpr` is 96 B
        // on a 32-bit target, which under the old flat 64-element bound made a
        // 6,144 B chunk — bigger than the classic ESP32's whole remaining heap
        // at the point it OOM'd compiling a shader.
        assert!(CHUNK * size_of::<i32>() <= CHUNK_BYTES);
        assert!(ChunkedVec::<[u8; 96]>::CHUNK_SIZE * 96 <= CHUNK_BYTES);
        assert!(ChunkedVec::<[u8; 20]>::CHUNK_SIZE * 20 <= CHUNK_BYTES);
        // An element larger than the whole budget still gets a chunk, not a
        // divide-by-zero.
        assert_eq!(ChunkedVec::<[u8; CHUNK_BYTES * 2]>::CHUNK_SIZE, 1);
    }

    #[test]
    fn push_and_len() {
        let mut v = chunked_vec_new();
        assert_eq!(v.len(), 0);
        v.push(1);
        v.push(2);
        v.push(3);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], 1);
        assert_eq!(v[1], 2);
        assert_eq!(v[2], 3);
        v.assert_inner_vecs_within_limit();
    }

    #[test]
    fn with_capacity_and_default() {
        let v = ChunkedVec::with_capacity_and_default(CHUNK * 2 + 2, -1);
        assert_eq!(v.len(), CHUNK * 2 + 2);
        for i in 0..CHUNK * 2 + 2 {
            assert_eq!(v[i], -1);
        }
        v.assert_inner_vecs_within_limit();
    }

    #[test]
    fn get_and_get_mut() {
        let mut v = chunked_vec_new();
        v.push(10);
        v.push(20);
        assert_eq!(v.get(0), Some(&10));
        assert_eq!(v.get(1), Some(&20));
        assert_eq!(v.get(2), None);
        *v.get_mut(1).unwrap() = 99;
        assert_eq!(v[1], 99);
    }

    #[test]
    fn resize_grow() {
        let mut v = chunked_vec_new();
        v.resize(5, -1);
        assert_eq!(v.len(), 5);
        for i in 0..5 {
            assert_eq!(v[i], -1);
        }
    }

    #[test]
    fn resize_shrink() {
        let mut v = chunked_vec_new();
        v.resize(10, 0);
        v.resize(3, 0);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn iter_yields_sequence() {
        let mut v = chunked_vec_new();
        for i in 0..10 {
            v.push(i as i32);
        }
        let collected: Vec<i32> = v.iter().copied().collect();
        assert_eq!(collected, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn reverse_preserves_layout_and_order() {
        let mut v = chunked_vec_new();
        let n = CHUNK * 2 + 2;
        for i in 0..n {
            v.push(i as i32);
        }
        v.reverse();
        for i in 0..n {
            assert_eq!(v[i], (n - 1 - i) as i32, "index {i} after reverse");
        }
        v.assert_inner_vecs_within_limit();
    }

    #[test]
    fn reverse_small_partial_chunk() {
        let mut v = chunked_vec_new();
        let n = CHUNK + 6;
        for i in 0..n {
            v.push(i as i32);
        }
        v.reverse();
        for i in 0..n {
            assert_eq!(v[i], (n - 1 - i) as i32);
        }
    }

    #[test]
    fn with_capacity_then_many_pushes() {
        let n = CHUNK * 9 + 24;
        let mut v = ChunkedVec::with_capacity(n);
        for i in 0..n {
            v.push(i as i32);
        }
        assert_eq!(v.len(), n);
        assert_eq!(v[0], 0);
        assert_eq!(v[n / 2], (n / 2) as i32);
        assert_eq!(v[n - 1], (n - 1) as i32);
        v.assert_inner_vecs_within_limit();
    }

    #[test]
    fn swap_remove() {
        let mut v = chunked_vec_new();
        for i in 0..5 {
            v.push(i as i32);
        }
        let removed = v.swap_remove(1);
        assert_eq!(removed, Some(1));
        assert_eq!(v.len(), 4);
        assert_eq!(v[0], 0);
        assert_eq!(v[1], 4);
        assert_eq!(v[2], 2);
        assert_eq!(v[3], 3);
    }

    #[test]
    fn binary_search_by() {
        let mut v = chunked_vec_new();
        for i in [1, 3, 5, 7, 9] {
            v.push(i);
        }
        assert_eq!(v.binary_search_by(|x| x.cmp(&5)), Ok(2));
        assert_eq!(v.binary_search_by(|x| x.cmp(&4)), Err(2));
        assert_eq!(v.binary_search_by(|x| x.cmp(&0)), Err(0));
        assert_eq!(v.binary_search_by(|x| x.cmp(&10)), Err(5));
    }

    #[test]
    fn iter_from() {
        let mut v = chunked_vec_new();
        for i in 0..25 {
            v.push(i as i32);
        }
        let from_10: Vec<i32> = v.iter_from(10).copied().collect();
        assert_eq!(from_10, (10..25).collect::<Vec<_>>());
        let from_0: Vec<i32> = v.iter_from(0).copied().collect();
        assert_eq!(from_0, (0..25).collect::<Vec<_>>());
    }

    #[test]
    fn sort_by_key() {
        let mut v = chunked_vec_new();
        v.push(30);
        v.push(10);
        v.push(20);
        v.sort_by_key(|&x| x);
        assert_eq!(v[0], 10);
        assert_eq!(v[1], 20);
        assert_eq!(v[2], 30);
        v.assert_inner_vecs_within_limit();
    }

    #[test]
    fn inner_vecs_stay_under_limit_push_across_boundaries() {
        let mut v = chunked_vec_new();
        for i in 0..CHUNK * 3 + 8 {
            v.push(i as i32);
            v.assert_inner_vecs_within_limit();
        }
    }

    #[test]
    fn inner_vecs_stay_under_limit_resize() {
        let mut v = chunked_vec_new();
        v.resize(CHUNK * 2 + 22, 0);
        v.assert_inner_vecs_within_limit();
        v.resize(CHUNK / 2, 0);
        v.assert_inner_vecs_within_limit();
    }

    #[test]
    fn inner_vecs_stay_under_limit_swap_remove() {
        let mut v = chunked_vec_new();
        for i in 0..CHUNK + 36 {
            v.push(i as i32);
        }
        for _ in 0..CHUNK / 2 {
            v.swap_remove(v.len() / 2);
            v.assert_inner_vecs_within_limit();
        }
    }
}

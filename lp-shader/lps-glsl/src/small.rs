//! Inline-small element lists for the compile hot path.
//!
//! Lowering builds one lane list per lowered expression and one IR-type list
//! per typed value. Nearly all of them hold 1-4 elements (scalar / vec2 /
//! vec3 / vec4), so a heap `Vec` per value is pure allocator churn on a
//! device that JITs shaders at runtime. [`InlineVec`] keeps the first `N`
//! elements on the stack and only spills to the heap for the wide shapes
//! (matrices, arrays, structs).

use alloc::vec::Vec;
use core::fmt;
use core::ops::Deref;

use lpir::{IrType, VReg};

/// Filler for the unused inline slots.
///
/// Keeps [`InlineVec`] free of `unsafe` and of a `Default` bound that the IR
/// value types do not carry. The filler is never observable: it only ever
/// occupies slots past `len`.
pub trait InlineFill: Copy {
    const FILL: Self;
}

impl InlineFill for VReg {
    const FILL: Self = VReg(0);
}

impl InlineFill for IrType {
    const FILL: Self = IrType::I32;
}

/// A `Vec`-like append-only list that keeps its first `N` elements inline.
///
/// `N` must be at most 255 — the inline length is a `u8` so that the inline
/// variant stays as small as possible.
pub enum InlineVec<T: InlineFill, const N: usize> {
    Inline { len: u8, buf: [T; N] },
    Heap(Vec<T>),
}

impl<T: InlineFill, const N: usize> InlineVec<T, N> {
    pub fn new() -> Self {
        debug_assert!(N <= u8::MAX as usize);
        Self::Inline {
            len: 0,
            buf: [T::FILL; N],
        }
    }

    /// A one-element list — the shape every scalar lowering produces.
    pub fn one(value: T) -> Self {
        let mut out = Self::new();
        out.push(value);
        out
    }

    pub fn from_slice(values: &[T]) -> Self {
        if values.len() <= N {
            let mut buf = [T::FILL; N];
            buf[..values.len()].copy_from_slice(values);
            Self::Inline {
                len: values.len() as u8,
                buf,
            }
        } else {
            Self::Heap(values.to_vec())
        }
    }

    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::Inline { len, buf } => &buf[..*len as usize],
            Self::Heap(heap) => heap.as_slice(),
        }
    }

    pub fn push(&mut self, value: T) {
        match self {
            Self::Inline { len, buf } if (*len as usize) < N => {
                buf[*len as usize] = value;
                *len += 1;
            }
            Self::Inline { len, buf } => {
                let mut heap = Vec::with_capacity(N * 2);
                heap.extend_from_slice(&buf[..*len as usize]);
                heap.push(value);
                *self = Self::Heap(heap);
            }
            Self::Heap(heap) => heap.push(value),
        }
    }

    pub fn truncate(&mut self, new_len: usize) {
        match self {
            Self::Inline { len, .. } => {
                if new_len < *len as usize {
                    *len = new_len as u8;
                }
            }
            Self::Heap(heap) => heap.truncate(new_len),
        }
    }

    pub fn resize(&mut self, new_len: usize, value: T) {
        while self.len() < new_len {
            self.push(value);
        }
        self.truncate(new_len);
    }
}

impl<T: InlineFill, const N: usize> Default for InlineVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: InlineFill, const N: usize> Clone for InlineVec<T, N> {
    fn clone(&self) -> Self {
        match self {
            Self::Inline { len, buf } => Self::Inline {
                len: *len,
                buf: *buf,
            },
            Self::Heap(heap) => Self::Heap(heap.clone()),
        }
    }
}

impl<T: InlineFill, const N: usize> Deref for InlineVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: InlineFill + fmt::Debug, const N: usize> fmt::Debug for InlineVec<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<T: InlineFill, const N: usize> Extend<T> for InlineVec<T, N> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value);
        }
    }
}

impl<T: InlineFill, const N: usize> FromIterator<T> for InlineVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut out = Self::new();
        out.extend(iter);
        out
    }
}

pub struct InlineVecIter<T: InlineFill, const N: usize> {
    values: InlineVec<T, N>,
    index: usize,
}

impl<T: InlineFill, const N: usize> Iterator for InlineVecIter<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        let value = *self.values.as_slice().get(self.index)?;
        self.index += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.values.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl<T: InlineFill, const N: usize> ExactSizeIterator for InlineVecIter<T, N> {}

impl<T: InlineFill, const N: usize> IntoIterator for InlineVec<T, N> {
    type Item = T;
    type IntoIter = InlineVecIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        InlineVecIter {
            values: self,
            index: 0,
        }
    }
}

impl<'a, T: InlineFill, const N: usize> IntoIterator for &'a InlineVec<T, N> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_until_capacity_then_spills() {
        let mut lanes = InlineVec::<VReg, 4>::new();
        for i in 0..4 {
            lanes.push(VReg(i));
        }
        assert!(matches!(lanes, InlineVec::Inline { len: 4, .. }));
        lanes.push(VReg(4));
        assert!(matches!(lanes, InlineVec::Heap(_)));
        assert_eq!(
            lanes.as_slice(),
            &[VReg(0), VReg(1), VReg(2), VReg(3), VReg(4)]
        );
    }

    #[test]
    fn truncate_and_resize_match_vec_semantics() {
        let mut lanes = InlineVec::<VReg, 4>::from_slice(&[VReg(7)]);
        lanes.resize(3, VReg(9));
        assert_eq!(lanes.as_slice(), &[VReg(7), VReg(9), VReg(9)]);
        lanes.resize(6, VReg(1));
        assert_eq!(lanes.len(), 6);
        lanes.truncate(2);
        assert_eq!(lanes.as_slice(), &[VReg(7), VReg(9)]);
        lanes.truncate(9);
        assert_eq!(lanes.len(), 2);
    }

    #[test]
    fn round_trips_through_iterators() {
        let lanes = (0..6).map(VReg).collect::<InlineVec<VReg, 4>>();
        let collected = lanes.clone().into_iter().collect::<Vec<_>>();
        assert_eq!(collected.len(), 6);
        assert_eq!(lanes.iter().copied().collect::<Vec<_>>(), collected);
    }
}

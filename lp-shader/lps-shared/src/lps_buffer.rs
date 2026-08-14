//! Packed word buffers at the shader ABI: the marshal-side twin of the
//! model's `LpBuffer`.
//!
//! A buffer crosses the ABI as one flat run of 4-byte words (f32 lanes as
//! `f32::to_bits`), so uniform fill and global read-back stay memcpy-class
//! instead of boxing one enum per element. Target layout padding (std430
//! vec3 stride) is applied at the byte read/write seams, never stored.

use alloc::boxed::Box;

use crate::LpsType;

/// Element descriptor: scalar kind × lanes, the closed buffer-legal set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LpsBufferElem {
    F32,
    Vec2,
    Vec3,
    Vec4,
    U32,
    UVec2,
    UVec3,
    UVec4,
    I32,
    IVec2,
    IVec3,
    IVec4,
}

impl LpsBufferElem {
    pub fn lanes(self) -> u32 {
        match self {
            Self::F32 | Self::U32 | Self::I32 => 1,
            Self::Vec2 | Self::UVec2 | Self::IVec2 => 2,
            Self::Vec3 | Self::UVec3 | Self::IVec3 => 3,
            Self::Vec4 | Self::UVec4 | Self::IVec4 => 4,
        }
    }

    /// Packed words per element (no padding in the stored form).
    pub fn word_stride(self) -> u32 {
        self.lanes()
    }

    /// Whether lanes carry f32 bits (vs integer words). The f32 kinds are
    /// the ones a Q32 target transcodes at the byte seam.
    pub fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::Vec2 | Self::Vec3 | Self::Vec4)
    }

    pub fn to_lps_type(self) -> LpsType {
        match self {
            Self::F32 => LpsType::Float,
            Self::Vec2 => LpsType::Vec2,
            Self::Vec3 => LpsType::Vec3,
            Self::Vec4 => LpsType::Vec4,
            Self::U32 => LpsType::UInt,
            Self::UVec2 => LpsType::UVec2,
            Self::UVec3 => LpsType::UVec3,
            Self::UVec4 => LpsType::UVec4,
            Self::I32 => LpsType::Int,
            Self::IVec2 => LpsType::IVec2,
            Self::IVec3 => LpsType::IVec3,
            Self::IVec4 => LpsType::IVec4,
        }
    }

    pub fn from_lps_type(ty: &LpsType) -> Option<Self> {
        Some(match ty {
            LpsType::Float => Self::F32,
            LpsType::Vec2 => Self::Vec2,
            LpsType::Vec3 => Self::Vec3,
            LpsType::Vec4 => Self::Vec4,
            LpsType::UInt => Self::U32,
            LpsType::UVec2 => Self::UVec2,
            LpsType::UVec3 => Self::UVec3,
            LpsType::UVec4 => Self::UVec4,
            LpsType::Int => Self::I32,
            LpsType::IVec2 => Self::IVec2,
            LpsType::IVec3 => Self::IVec3,
            LpsType::IVec4 => Self::IVec4,
            _ => return None,
        })
    }
}

/// A packed buffer value: element descriptor plus the flat word run.
/// Invariant: `words.len()` is a multiple of `elem.word_stride()`.
#[derive(Debug, Clone)]
pub struct LpsBuffer {
    pub elem: LpsBufferElem,
    words: Box<[u32]>,
}

impl LpsBuffer {
    pub fn zeroed(elem: LpsBufferElem, len: u32) -> Self {
        Self {
            elem,
            words: alloc::vec![0u32; (len as usize) * elem.word_stride() as usize]
                .into_boxed_slice(),
        }
    }

    pub fn from_words(elem: LpsBufferElem, words: Box<[u32]>) -> Result<Self, &'static str> {
        if words.len() % elem.word_stride() as usize != 0 {
            return Err("buffer word count is not a multiple of the element stride");
        }
        Ok(Self { elem, words })
    }

    /// Logical element count (`words / stride`).
    pub fn len(&self) -> u32 {
        (self.words.len() / self.elem.word_stride() as usize) as u32
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn words(&self) -> &[u32] {
        &self.words
    }

    /// Bit-exact equality: two buffers are equal iff elem and words match.
    pub fn bits_eq(&self, other: &Self) -> bool {
        self.elem == other.elem && self.words == other.words
    }

    /// Per-lane approximate equality: f32 lanes within `tolerance`
    /// (bit-equal short-circuits, so identical NaN/inf pass), integer
    /// lanes exact.
    pub fn approx_eq(&self, other: &Self, tolerance: f32) -> bool {
        if self.elem != other.elem || self.words.len() != other.words.len() {
            return false;
        }
        if !self.elem.is_float() {
            return self.words == other.words;
        }
        self.words.iter().zip(other.words.iter()).all(|(a, b)| {
            a == b || {
                let (a, b) = (f32::from_bits(*a), f32::from_bits(*b));
                (a - b).abs() <= tolerance
            }
        })
    }
}

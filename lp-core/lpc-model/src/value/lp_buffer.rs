//! Packed typed buffers: dense arrays of builtin scalars/vectors.
//!
//! A buffer is the memory-honest form of per-cell shader state
//! (`float heat[N];`): one flat run of little-endian 4-byte words, no
//! per-element enum boxing, no padding. Target-specific layout padding
//! (std430 vec3 stride, std140 uniform strides) belongs to the ABI
//! marshal layer and is never stored. See
//! `docs/adr/2026-08-08-typed-shader-buffers.md`.

use alloc::vec;
use alloc::vec::Vec;

use crate::LpType;

/// Element type of a packed buffer: scalar kind × lane count.
///
/// A deliberate closed set — `Box<LpType>` here would admit buffers of
/// strings and structs, which have no packed word form. Bool and matrix
/// elements are excluded until something needs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BufferElem {
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

/// Scalar interpretation of a buffer's lanes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferScalar {
    F32,
    U32,
    I32,
}

impl BufferElem {
    /// Lanes per element (1 for scalars, 2–4 for vectors).
    pub fn lanes(self) -> u32 {
        match self {
            Self::F32 | Self::U32 | Self::I32 => 1,
            Self::Vec2 | Self::UVec2 | Self::IVec2 => 2,
            Self::Vec3 | Self::UVec3 | Self::IVec3 => 3,
            Self::Vec4 | Self::UVec4 | Self::IVec4 => 4,
        }
    }

    /// Packed words per element. Identical to [`Self::lanes`] because the
    /// stored form has no padding — the name exists so call sites say
    /// which of the two facts they rely on.
    pub fn word_stride(self) -> u32 {
        self.lanes()
    }

    pub fn scalar(self) -> BufferScalar {
        match self {
            Self::F32 | Self::Vec2 | Self::Vec3 | Self::Vec4 => BufferScalar::F32,
            Self::U32 | Self::UVec2 | Self::UVec3 | Self::UVec4 => BufferScalar::U32,
            Self::I32 | Self::IVec2 | Self::IVec3 | Self::IVec4 => BufferScalar::I32,
        }
    }

    /// The GLSL type this element declares (`float heat[N];`).
    pub fn glsl_name(self) -> &'static str {
        match self {
            Self::F32 => "float",
            Self::Vec2 => "vec2",
            Self::Vec3 => "vec3",
            Self::Vec4 => "vec4",
            Self::U32 => "uint",
            Self::UVec2 => "uvec2",
            Self::UVec3 => "uvec3",
            Self::UVec4 => "uvec4",
            Self::I32 => "int",
            Self::IVec2 => "ivec2",
            Self::IVec3 => "ivec3",
            Self::IVec4 => "ivec4",
        }
    }

    pub fn as_lp_type(self) -> LpType {
        match self {
            Self::F32 => LpType::F32,
            Self::Vec2 => LpType::Vec2,
            Self::Vec3 => LpType::Vec3,
            Self::Vec4 => LpType::Vec4,
            Self::U32 => LpType::U32,
            Self::UVec2 => LpType::UVec2,
            Self::UVec3 => LpType::UVec3,
            Self::UVec4 => LpType::UVec4,
            Self::I32 => LpType::I32,
            Self::IVec2 => LpType::IVec2,
            Self::IVec3 => LpType::IVec3,
            Self::IVec4 => LpType::IVec4,
        }
    }

    pub fn from_lp_type(ty: &LpType) -> Option<Self> {
        Some(match ty {
            LpType::F32 => Self::F32,
            LpType::Vec2 => Self::Vec2,
            LpType::Vec3 => Self::Vec3,
            LpType::Vec4 => Self::Vec4,
            LpType::U32 => Self::U32,
            LpType::UVec2 => Self::UVec2,
            LpType::UVec3 => Self::UVec3,
            LpType::UVec4 => Self::UVec4,
            LpType::I32 => Self::I32,
            LpType::IVec2 => Self::IVec2,
            LpType::IVec3 => Self::IVec3,
            LpType::IVec4 => Self::IVec4,
            _ => return None,
        })
    }
}

/// A packed typed buffer value: element descriptor plus the flat word run.
///
/// f32 lanes are stored as `f32::to_bits` words, which is what makes
/// equality (`PartialEq` on words) bit-exact and deterministic — two
/// buffers are equal iff their bytes are, with no NaN or -0.0 ambiguity.
/// Invariant: `words.len()` is a multiple of `elem.word_stride()`; the
/// logical length is derived, never stored separately.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct LpBuffer {
    pub elem: BufferElem,
    words: Vec<u32>,
}

impl LpBuffer {
    /// A buffer of `len` zeroed elements.
    pub fn zeroed(elem: BufferElem, len: u32) -> Self {
        Self {
            elem,
            words: vec![0; (len as usize) * elem.word_stride() as usize],
        }
    }

    /// Construct from raw words. Fails when the word count is not a
    /// multiple of the element stride.
    pub fn from_words(elem: BufferElem, words: Vec<u32>) -> Result<Self, LpBufferError> {
        if words.len() % elem.word_stride() as usize != 0 {
            return Err(LpBufferError::WordCount {
                words: words.len(),
                stride: elem.word_stride(),
            });
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

    pub fn words_mut(&mut self) -> &mut [u32] {
        &mut self.words
    }

    pub fn byte_len(&self) -> usize {
        self.words.len() * 4
    }

    /// The canonical byte form: each word little-endian. This is the
    /// exact payload the base64 JSON encoding carries.
    pub fn to_le_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.byte_len());
        for word in &self.words {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// Decode the canonical byte form. Fails on a length that is not a
    /// whole number of elements.
    pub fn from_le_bytes(elem: BufferElem, bytes: &[u8]) -> Result<Self, LpBufferError> {
        if bytes.len() % 4 != 0 {
            return Err(LpBufferError::ByteCount { bytes: bytes.len() });
        }
        let mut words = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Self::from_words(elem, words)
    }
}

/// Failure constructing an [`LpBuffer`] from raw words or bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LpBufferError {
    WordCount { words: usize, stride: u32 },
    ByteCount { bytes: usize },
}

impl core::fmt::Display for LpBufferError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WordCount { words, stride } => write!(
                f,
                "buffer word count {words} is not a multiple of the element stride {stride}"
            ),
            Self::ByteCount { bytes } => {
                write!(f, "buffer byte count {bytes} is not a multiple of 4")
            }
        }
    }
}

impl core::error::Error for LpBufferError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeroed_buffer_has_len_times_stride_words() {
        let buffer = LpBuffer::zeroed(BufferElem::Vec3, 5);
        assert_eq!(buffer.len(), 5);
        assert_eq!(buffer.words().len(), 15);
    }

    #[test]
    fn le_bytes_round_trip_is_bit_exact() {
        let buffer = LpBuffer::from_words(
            BufferElem::F32,
            vec![
                f32::to_bits(1.5),
                f32::to_bits(-0.0),
                f32::to_bits(f32::NAN),
            ],
        )
        .expect("buffer");

        let back = LpBuffer::from_le_bytes(BufferElem::F32, &buffer.to_le_bytes()).expect("back");

        assert_eq!(back, buffer);
    }

    #[test]
    fn from_words_rejects_partial_elements() {
        assert!(matches!(
            LpBuffer::from_words(BufferElem::Vec2, vec![1, 2, 3]),
            Err(LpBufferError::WordCount { .. })
        ));
    }
}

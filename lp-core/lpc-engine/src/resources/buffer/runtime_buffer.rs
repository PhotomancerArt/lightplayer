//! Runtime-owned buffers with domain metadata (texture, fixture colors, output, raw).
//!
//! A buffer's payload is stored in the element type its producer renders
//! ([`RuntimeBufferData`]): bytes for textures, colors and raw payloads,
//! `u16` samples for output channels. The wire form is always bytes
//! ([`RuntimeBuffer::bytes`] encodes `u16` samples little-endian), but storing
//! output samples as `u16` is what lets the output node render straight into
//! its channel buffer and the flush path borrow them — one home per lamp
//! colour instead of a render target, an LE byte copy, and a decode back
//! (`docs/reports/2026-09-02-per-lamp-memory-table.md`).

use alloc::borrow::Cow;
use alloc::vec::Vec;

/// High-level classification of buffer payloads in [`RuntimeBuffer`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeBufferKind {
    Texture,
    FixtureColors,
    OutputChannels,
    Raw,
}

/// Pixel / channel format for texture buffers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeTextureFormat {
    Rgba16,
    Rgb8,
}

/// Memory layout for fixture color bytes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeColorLayout {
    Rgb8,
}

/// Element format for output channel samples in a [`RuntimeBuffer`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeChannelSampleFormat {
    U8,
    U16,
}

/// Per-domain metadata describing how to interpret a [`RuntimeBuffer`]'s payload.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeBufferMetadata {
    Texture {
        width: u32,
        height: u32,
        format: RuntimeTextureFormat,
    },
    FixtureColors {
        channels: u32,
        layout: RuntimeColorLayout,
    },
    OutputChannels {
        channels: u32,
        sample_format: RuntimeChannelSampleFormat,
    },
    Raw,
}

/// A buffer's payload, in the element type its producer renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeBufferData {
    /// Opaque bytes: textures, fixture colors, raw payloads, 8-bit channels.
    Bytes(Vec<u8>),
    /// 16-bit output-channel samples; the wire form is little-endian pairs.
    Samples16(Vec<u16>),
}

impl RuntimeBufferData {
    /// The payload in its wire (byte) form — borrowed for bytes, encoded
    /// little-endian for `u16` samples.
    #[must_use]
    pub fn bytes(&self) -> Cow<'_, [u8]> {
        match self {
            Self::Bytes(bytes) => Cow::Borrowed(bytes.as_slice()),
            Self::Samples16(samples) => {
                let mut bytes = Vec::with_capacity(samples.len() * 2);
                for sample in samples {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                Cow::Owned(bytes)
            }
        }
    }

    /// Length of the wire (byte) form, without encoding it.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Samples16(samples) => samples.len() * 2,
        }
    }
}

/// Authoritative runtime buffer payload: kind, metadata, and contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeBuffer {
    pub kind: RuntimeBufferKind,
    pub metadata: RuntimeBufferMetadata,
    pub data: RuntimeBufferData,
}

impl RuntimeBuffer {
    #[must_use]
    pub fn texture_rgba16(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self {
            kind: RuntimeBufferKind::Texture,
            metadata: RuntimeBufferMetadata::Texture {
                width,
                height,
                format: RuntimeTextureFormat::Rgba16,
            },
            data: RuntimeBufferData::Bytes(bytes),
        }
    }

    #[must_use]
    pub fn fixture_colors_rgb8(channels: u32, bytes: Vec<u8>) -> Self {
        Self {
            kind: RuntimeBufferKind::FixtureColors,
            metadata: RuntimeBufferMetadata::FixtureColors {
                channels,
                layout: RuntimeColorLayout::Rgb8,
            },
            data: RuntimeBufferData::Bytes(bytes),
        }
    }

    #[must_use]
    pub fn output_channels_u8(channels: u32, bytes: Vec<u8>) -> Self {
        Self {
            kind: RuntimeBufferKind::OutputChannels,
            metadata: RuntimeBufferMetadata::OutputChannels {
                channels,
                sample_format: RuntimeChannelSampleFormat::U8,
            },
            data: RuntimeBufferData::Bytes(bytes),
        }
    }

    /// Output channels as `u16` samples (three per lamp).
    #[must_use]
    pub fn output_channels_u16(channels: u32, samples: Vec<u16>) -> Self {
        Self {
            kind: RuntimeBufferKind::OutputChannels,
            metadata: RuntimeBufferMetadata::OutputChannels {
                channels,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
            data: RuntimeBufferData::Samples16(samples),
        }
    }

    #[must_use]
    pub fn raw(bytes: Vec<u8>) -> Self {
        Self {
            kind: RuntimeBufferKind::Raw,
            metadata: RuntimeBufferMetadata::Raw,
            data: RuntimeBufferData::Bytes(bytes),
        }
    }

    /// The payload in its wire (byte) form. See [`RuntimeBufferData::bytes`].
    #[must_use]
    pub fn bytes(&self) -> Cow<'_, [u8]> {
        self.data.bytes()
    }

    /// Length of the wire (byte) form.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.data.byte_len()
    }

    /// The byte payload, for producers that render bytes; `None` for a
    /// `u16` sample buffer.
    pub fn bytes_mut(&mut self) -> Option<&mut Vec<u8>> {
        match &mut self.data {
            RuntimeBufferData::Bytes(bytes) => Some(bytes),
            RuntimeBufferData::Samples16(_) => None,
        }
    }

    /// The `u16` samples, when the payload is stored as samples.
    #[must_use]
    pub fn samples16(&self) -> Option<&[u16]> {
        match &self.data {
            RuntimeBufferData::Samples16(samples) => Some(samples.as_slice()),
            RuntimeBufferData::Bytes(_) => None,
        }
    }

    /// Move the `u16` samples out, leaving an empty sample buffer behind —
    /// how the output node borrows its channel buffer's storage for the
    /// frame it renders (and hands it back with [`Self::set_samples16`]).
    /// `None` when the payload is bytes.
    pub fn take_samples16(&mut self) -> Option<Vec<u16>> {
        match &mut self.data {
            RuntimeBufferData::Samples16(samples) => Some(core::mem::take(samples)),
            RuntimeBufferData::Bytes(_) => None,
        }
    }

    /// Store `samples` as the payload (replacing whatever form it had).
    pub fn set_samples16(&mut self, samples: Vec<u16>) {
        self.data = RuntimeBufferData::Samples16(samples);
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{
        RuntimeBuffer, RuntimeBufferKind, RuntimeBufferMetadata, RuntimeChannelSampleFormat,
        RuntimeColorLayout,
    };

    #[test]
    fn fixture_colors_helper_sets_kind_and_metadata() {
        let b = RuntimeBuffer::fixture_colors_rgb8(12, vec![0, 1, 2]);
        assert_eq!(b.kind, RuntimeBufferKind::FixtureColors);
        assert_eq!(
            b.metadata,
            RuntimeBufferMetadata::FixtureColors {
                channels: 12,
                layout: RuntimeColorLayout::Rgb8,
            }
        );
        assert_eq!(b.bytes().as_ref(), &[0, 1, 2]);
        assert_eq!(b.byte_len(), 3);
        assert!(b.samples16().is_none());
    }

    #[test]
    fn output_channels_helper_sets_kind_and_metadata() {
        let b = RuntimeBuffer::output_channels_u8(4, vec![10, 20]);
        assert_eq!(b.kind, RuntimeBufferKind::OutputChannels);
        assert_eq!(
            b.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 4,
                sample_format: RuntimeChannelSampleFormat::U8,
            }
        );
    }

    #[test]
    fn output_channels_u16_helper_sets_kind_and_metadata() {
        let b = RuntimeBuffer::output_channels_u16(4, vec![10, 20]);
        assert_eq!(b.kind, RuntimeBufferKind::OutputChannels);
        assert_eq!(
            b.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 4,
                sample_format: RuntimeChannelSampleFormat::U16,
            }
        );
        assert_eq!(b.samples16(), Some(&[10u16, 20][..]));
    }

    /// The wire form of a `u16` sample buffer is the little-endian pair
    /// encoding it always was — a client reading published frames sees the
    /// same bytes whether the engine stores samples or bytes.
    #[test]
    fn u16_samples_encode_to_little_endian_bytes() {
        let b = RuntimeBuffer::output_channels_u16(1, vec![0x0102, 0xFFFF, 1]);
        assert_eq!(b.bytes().as_ref(), &[0x02, 0x01, 0xFF, 0xFF, 1, 0]);
        assert_eq!(b.byte_len(), 6);
    }

    #[test]
    fn take_and_set_samples_move_the_storage_without_copying() {
        let mut b = RuntimeBuffer::output_channels_u16(1, vec![1, 2, 3]);
        let taken = b.take_samples16().expect("sample buffer");
        assert_eq!(taken, vec![1, 2, 3]);
        assert_eq!(b.samples16(), Some(&[][..]));
        b.set_samples16(taken);
        assert_eq!(b.samples16(), Some(&[1u16, 2, 3][..]));
        assert!(RuntimeBuffer::raw(vec![9]).take_samples16().is_none());
    }
}

//! Runtime-owned byte buffers with domain metadata (texture, fixture colors, output, raw).

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

/// Element format for output channel samples in [`RuntimeBuffer::bytes`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeChannelSampleFormat {
    U8,
    U16,
}

/// Per-domain metadata describing how to interpret [`RuntimeBuffer::bytes`].
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

/// Authoritative runtime buffer payload: kind, metadata, and byte contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeBuffer {
    pub kind: RuntimeBufferKind,
    pub metadata: RuntimeBufferMetadata,
    pub bytes: Vec<u8>,
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
            bytes,
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
            bytes,
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
            bytes,
        }
    }

    #[must_use]
    pub fn output_channels_u16(channels: u32, bytes: Vec<u8>) -> Self {
        Self {
            kind: RuntimeBufferKind::OutputChannels,
            metadata: RuntimeBufferMetadata::OutputChannels {
                channels,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
            bytes,
        }
    }

    #[must_use]
    pub fn raw(bytes: Vec<u8>) -> Self {
        Self {
            kind: RuntimeBufferKind::Raw,
            metadata: RuntimeBufferMetadata::Raw,
            bytes,
        }
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
        assert_eq!(b.bytes, vec![0, 1, 2]);
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
    }
}

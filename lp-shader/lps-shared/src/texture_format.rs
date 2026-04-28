//! CPU-side texture storage format (single source of truth for layout).

/// Storage format for CPU-side texture data.
///
/// Variants are added when there is a concrete consumer; each variant documents
/// layout and channel semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureStorageFormat {
    /// RGBA 16-bit unsigned normalized, 8 bytes/pixel.
    ///
    /// Each channel is a `u16` in `[0, 65535]` representing `[0.0, 1.0]`.
    /// Q32 fractional bits map to unorm16 via saturate: `min(q32, 65535)`.
    Rgba16Unorm,
    /// RGB 16-bit unsigned normalized, 6 bytes/pixel (no alpha).
    ///
    /// Tightly packed: 3 × u16 = 6 bytes per pixel. No padding.
    Rgb16Unorm,
    /// Single-channel 16-bit unsigned normalized, 2 bytes/pixel.
    R16Unorm,
}

impl TextureStorageFormat {
    #[inline]
    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba16Unorm => 8,
            Self::Rgb16Unorm => 6,
            Self::R16Unorm => 2,
        }
    }

    #[inline]
    #[must_use]
    pub fn channel_count(self) -> usize {
        match self {
            Self::Rgba16Unorm => 4,
            Self::Rgb16Unorm => 3,
            Self::R16Unorm => 1,
        }
    }

    /// Minimum alignment (bytes) required for loads on this GPU texture path (base pointer, row stride,
    /// and channel column offsets relative to row starts).
    ///
    /// Current 16-bit unorm formats use halfword loads, so they require 2-byte alignment.
    ///
    /// Future `R8`/`u8`-backed formats should be able to return 1. Future fully 32-bit channel
    /// layouts can require 4 without changing the guest [`LpsTexture2DDescriptor`] ABI.
    #[inline]
    #[must_use]
    pub fn required_load_alignment(self) -> usize {
        match self {
            Self::Rgba16Unorm | Self::Rgb16Unorm | Self::R16Unorm => 2,
        }
    }
}

/// Compile-time filter mode for a texture binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

/// Edge sampling mode on one axis of a 2D texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureWrap {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

/// Optional shape hint for validation or lowering (2D vs 1D-along-y strip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureShapeHint {
    General2D,
    HeightOne,
}

/// Full compile-time description of a 2D texture binding (format + sampling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureBindingSpec {
    pub format: TextureStorageFormat,
    pub filter: TextureFilter,
    pub wrap_x: TextureWrap,
    pub wrap_y: TextureWrap,
    pub shape_hint: TextureShapeHint,
}

/// Guest std430 ABI for [`crate::LpsType::Texture2D`]: one pointer plus layout (`u32` lanes).
///
/// This is a role-neutral opaque descriptor: the same value can be carried in [`crate::LpsValueF32`]
/// and [`crate::LpsValueQ32`] and passed as four raw `i32` lanes where the calling convention
/// allows it, independent of “uniform” vs “parameter” GLSL address spaces.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LpsTexture2DDescriptor {
    pub ptr: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}

/// Host-side 2D texture value: guest [`LpsTexture2DDescriptor`] plus storage facts for runtime validation.
///
/// Only [`LpsTexture2DDescriptor`] participates in the four-lane guest ABI; `format` and `byte_len` are
/// not written into LPVM uniform memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LpsTexture2DValue {
    pub descriptor: LpsTexture2DDescriptor,
    pub format: TextureStorageFormat,
    /// Backing allocation size in bytes when known from a host buffer allocation.
    ///
    /// Zero means storage size was not supplied (e.g. value rehydrated from guest uniform bytes only).
    /// Runtime validation must not treat `format` / `byte_len` as authoritative when `byte_len == 0`.
    pub byte_len: usize,
}

impl LpsTexture2DValue {
    /// Build a value from raw guest descriptor lanes when host storage metadata is unavailable.
    ///
    /// Sets [`LpsTexture2DValue::byte_len`] to zero and uses a placeholder `format` that is not
    /// stored in the guest layout; do not use for validation until the value is replaced with a
    /// buffer-backed [`LpsTexture2DValue`].
    #[must_use]
    pub fn from_guest_descriptor(descriptor: LpsTexture2DDescriptor) -> Self {
        Self {
            descriptor,
            // Placeholder: not encoded in guest uniforms; pair with `byte_len == 0`.
            format: TextureStorageFormat::R16Unorm,
            byte_len: 0,
        }
    }

    /// Bytes required to store the image with `descriptor.row_stride` bytes between row starts:
    /// `row_stride * (height - 1) + width * bytes_per_pixel` (last row may be shorter than `row_stride`).
    ///
    /// Returns [`None`] on overflow or when `width`/`height` are zero.
    #[must_use]
    pub fn required_footprint_bytes(self) -> Option<u64> {
        required_texture_footprint_bytes(self.format, self.descriptor)
    }
}

fn required_texture_footprint_bytes(
    format: TextureStorageFormat,
    d: LpsTexture2DDescriptor,
) -> Option<u64> {
    if d.width == 0 || d.height == 0 {
        return None;
    }
    let bpp = u64::try_from(format.bytes_per_pixel()).ok()?;
    let w = u64::from(d.width);
    let h = u64::from(d.height);
    let row_stride = u64::from(d.row_stride);
    let last_row_bytes = w.checked_mul(bpp)?;
    let padded_rows = if h <= 1 {
        0u64
    } else {
        row_stride.checked_mul(h - 1)?
    };
    padded_rows.checked_add(last_row_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_binding_spec_construct_and_compare() {
        let a = TextureBindingSpec {
            format: TextureStorageFormat::Rgba16Unorm,
            filter: TextureFilter::Linear,
            wrap_x: TextureWrap::Repeat,
            wrap_y: TextureWrap::ClampToEdge,
            shape_hint: TextureShapeHint::HeightOne,
        };
        let b = TextureBindingSpec {
            format: TextureStorageFormat::Rgba16Unorm,
            filter: TextureFilter::Linear,
            wrap_x: TextureWrap::Repeat,
            wrap_y: TextureWrap::ClampToEdge,
            shape_hint: TextureShapeHint::HeightOne,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn rgba16_unorm_bytes_per_pixel() {
        assert_eq!(TextureStorageFormat::Rgba16Unorm.bytes_per_pixel(), 8);
    }

    #[test]
    fn rgba16_unorm_channel_count() {
        assert_eq!(TextureStorageFormat::Rgba16Unorm.channel_count(), 4);
    }

    #[test]
    fn rgb16_unorm_metrics() {
        assert_eq!(TextureStorageFormat::Rgb16Unorm.bytes_per_pixel(), 6);
        assert_eq!(TextureStorageFormat::Rgb16Unorm.channel_count(), 3);
    }

    #[test]
    fn r16_unorm_metrics() {
        assert_eq!(TextureStorageFormat::R16Unorm.bytes_per_pixel(), 2);
        assert_eq!(TextureStorageFormat::R16Unorm.channel_count(), 1);
    }

    #[test]
    fn required_load_alignment_is_two_for_supported_unorm16_formats() {
        assert_eq!(
            TextureStorageFormat::Rgba16Unorm.required_load_alignment(),
            2
        );
        assert_eq!(
            TextureStorageFormat::Rgb16Unorm.required_load_alignment(),
            2
        );
        assert_eq!(TextureStorageFormat::R16Unorm.required_load_alignment(), 2);
    }

    #[test]
    fn required_footprint_accepts_padded_rows() {
        let d = LpsTexture2DDescriptor {
            ptr: 0,
            width: 2,
            height: 2,
            row_stride: 16,
        };
        let v = LpsTexture2DValue {
            descriptor: d,
            format: TextureStorageFormat::R16Unorm,
            byte_len: 64,
        };
        // bpp=2: last row is 4 bytes; first row padded prefix is (height-1)*row_stride = 16.
        assert_eq!(v.required_footprint_bytes(), Some(20));
    }

    #[test]
    fn required_footprint_rejects_overflow() {
        let v = LpsTexture2DValue {
            descriptor: LpsTexture2DDescriptor {
                ptr: 0,
                width: u32::MAX,
                height: u32::MAX,
                row_stride: u32::MAX,
            },
            format: TextureStorageFormat::Rgba16Unorm,
            byte_len: 0,
        };
        assert_eq!(v.required_footprint_bytes(), None);
    }

    #[test]
    fn required_footprint_rejects_zero_width_or_height() {
        let v = LpsTexture2DValue {
            descriptor: LpsTexture2DDescriptor {
                ptr: 0,
                width: 0,
                height: 1,
                row_stride: 2,
            },
            format: TextureStorageFormat::R16Unorm,
            byte_len: 0,
        };
        assert_eq!(v.required_footprint_bytes(), None);
    }
}

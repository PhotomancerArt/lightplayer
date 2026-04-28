//! Runtime validation of texture uniform bindings vs compile-time [`TextureBindingSpec`].

use alloc::format;

use lps_shared::{LpsTexture2DValue, TextureBindingSpec, TextureShapeHint, TextureStorageFormat};

use crate::error::LpsError;

/// Validate a single runtime `sampler2D` binding before descriptor lanes are written to the VM.
pub(crate) fn validate_runtime_texture_binding(
    name: &str,
    value: &LpsTexture2DValue,
    spec: &TextureBindingSpec,
) -> Result<(), LpsError> {
    if value.byte_len == 0 {
        return Err(render_err(
            name,
            "buffer-backed texture metadata is required (byte_len is zero); use LpsTextureBuf::to_texture2d_value() when binding",
        ));
    }

    if value.format != spec.format {
        return Err(render_err(
            name,
            format!(
                "format mismatch: shader expects {:?}, binding provides {:?}",
                spec.format, value.format
            ),
        ));
    }

    let d = &value.descriptor;
    if d.width == 0 || d.height == 0 {
        return Err(render_err(
            name,
            format!(
                "descriptor width and height must be positive (got {}x{})",
                d.width, d.height
            ),
        ));
    }

    if spec.shape_hint == TextureShapeHint::HeightOne && d.height != 1 {
        return Err(render_err(
            name,
            format!(
                "texture shape hint is height-one (expected height 1), descriptor height is {}",
                d.height
            ),
        ));
    }

    validate_layout(name, value, spec.format)?;

    let footprint = value.required_footprint_bytes().ok_or_else(|| {
        render_err(
            name,
            "could not compute texture footprint (overflow or invalid dimensions)",
        )
    })?;

    if footprint > u64::try_from(value.byte_len).unwrap_or(u64::MAX) {
        return Err(render_err(
            name,
            format!(
                "texture footprint ({} bytes) exceeds binding byte_len ({})",
                footprint, value.byte_len
            ),
        ));
    }

    Ok(())
}

fn validate_layout(
    name: &str,
    value: &LpsTexture2DValue,
    format: TextureStorageFormat,
) -> Result<(), LpsError> {
    let d = &value.descriptor;
    let align = format.required_load_alignment();
    let ptr = d.ptr as usize;
    if ptr % align != 0 {
        return Err(render_err(
            name,
            format!(
                "descriptor ptr must be {}-byte aligned for {:?} (ptr is {:#x})",
                align, format, d.ptr
            ),
        ));
    }

    let bpp = format.bytes_per_pixel();
    let min_row = (d.width as u64)
        .checked_mul(bpp as u64)
        .ok_or_else(|| render_err(name, "row stride minimum arithmetic overflow"))?;
    if (d.row_stride as u64) < min_row {
        return Err(render_err(
            name,
            format!(
                "row_stride ({}) is less than width * bytes_per_pixel ({}) for {:?}",
                d.row_stride, min_row, format
            ),
        ));
    }

    let rs = d.row_stride as usize;
    if rs % align != 0 {
        return Err(render_err(
            name,
            format!("row_stride ({rs}) must be {align}-byte aligned for {format:?}",),
        ));
    }

    Ok(())
}

fn render_err(name: &str, msg: impl core::fmt::Display) -> LpsError {
    LpsError::Render(format!("texture uniform `{name}`: {msg}"))
}

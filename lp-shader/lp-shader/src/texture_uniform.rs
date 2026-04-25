//! Guest ABI for passing a 2D texture reference as a uniform block.

use crate::texture_buf::LpsTextureBuf;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture2DUniform {
    pub ptr: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}

impl Texture2DUniform {
    /// Pack guest pointer, dimensions, and row stride from an allocated texture.
    #[must_use]
    pub fn from_texture(buf: &LpsTextureBuf) -> Self {
        let row = buf.row_stride();
        Self {
            ptr: buf.guest_ptr().guest_value() as u32,
            width: buf.width(),
            height: buf.height(),
            row_stride: row as u32,
        }
    }
}

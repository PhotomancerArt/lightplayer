//! Engine-time product of resolution: direct shader value or render-product handle.

use lps_shared::LpsValueF32;

use crate::render_product::RenderProductId;
use crate::runtime_buffer::RuntimeBufferId;

/// Building a [`RuntimeProduct`] from an invalid domain-specific value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeProductError {
    /// [`LpsValueF32::Texture2D`] is shader ABI only; use [`RuntimeProduct::Buffer`] or render handles.
    Texture2dValueNotRuntimeProduct,
}

impl core::fmt::Display for RuntimeProductError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Texture2dValueNotRuntimeProduct => {
                f.write_str("LpsValueF32::Texture2D cannot be wrapped in RuntimeProduct::Value")
            }
        }
    }
}

impl core::error::Error for RuntimeProductError {}

/// Payload for produced runtime values: GLSL-compatible data or engine-owned product handles.
#[derive(Debug, Clone)]
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProductId),
    Buffer(RuntimeBufferId),
}

impl RuntimeProduct {
    /// Wrap `value` as [`RuntimeProduct::Value`] unless it is [`LpsValueF32::Texture2D`].
    pub fn try_value(value: LpsValueF32) -> Result<Self, RuntimeProductError> {
        match value {
            LpsValueF32::Texture2D(_) => Err(RuntimeProductError::Texture2dValueNotRuntimeProduct),
            other => Ok(Self::Value(other)),
        }
    }

    /// Same as [`Self::try_value`]; prefer [`Self::try_value`] at call sites that handle errors.
    pub fn value(value: LpsValueF32) -> Result<Self, RuntimeProductError> {
        Self::try_value(value)
    }

    #[must_use]
    pub fn render(id: RenderProductId) -> Self {
        Self::Render(id)
    }

    #[must_use]
    pub fn buffer(id: RuntimeBufferId) -> Self {
        Self::Buffer(id)
    }

    pub fn as_value(&self) -> Option<&LpsValueF32> {
        match self {
            Self::Value(v) => Some(v),
            Self::Render(_) | Self::Buffer(_) => None,
        }
    }

    pub fn as_render(&self) -> Option<RenderProductId> {
        match self {
            Self::Render(id) => Some(*id),
            Self::Value(_) | Self::Buffer(_) => None,
        }
    }

    pub fn as_buffer(&self) -> Option<RuntimeBufferId> {
        match self {
            Self::Buffer(id) => Some(*id),
            Self::Value(_) | Self::Render(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use lps_shared::{LpsTexture2DDescriptor, LpsTexture2DValue, LpsValueF32};

    use super::{RuntimeProduct, RuntimeProductError};
    use crate::render_product::RenderProductId;
    use crate::runtime_buffer::RuntimeBufferId;

    #[test]
    fn runtime_product_value_helper_returns_value() {
        let p = RuntimeProduct::value(LpsValueF32::F32(3.14)).expect("scalar value");
        assert!(matches!(p.as_value(), Some(LpsValueF32::F32(_))));
        assert!(p.as_render().is_none());
        assert!(p.as_buffer().is_none());
    }

    #[test]
    fn runtime_product_render_helper_returns_id() {
        let id = RenderProductId::new(7);
        let p = RuntimeProduct::render(id);
        assert_eq!(p.as_render(), Some(id));
        assert!(p.as_value().is_none());
        assert!(p.as_buffer().is_none());
    }

    #[test]
    fn runtime_product_buffer_helper_returns_id() {
        let id = RuntimeBufferId::new(99);
        let p = RuntimeProduct::buffer(id);
        assert_eq!(p.as_buffer(), Some(id));
        assert!(p.as_value().is_none());
        assert!(p.as_render().is_none());
    }

    #[test]
    fn try_value_accepts_f32_rejects_texture2d() {
        assert!(matches!(
            RuntimeProduct::try_value(LpsValueF32::F32(1.0)),
            Ok(RuntimeProduct::Value(ref v)) if v.eq(&LpsValueF32::F32(1.0))
        ));
        let tv = LpsTexture2DValue::from_guest_descriptor(LpsTexture2DDescriptor {
            ptr: 0,
            width: 1,
            height: 1,
            row_stride: 4,
        });
        assert!(matches!(
            RuntimeProduct::try_value(LpsValueF32::Texture2D(tv)),
            Err(RuntimeProductError::Texture2dValueNotRuntimeProduct)
        ));
    }

    #[test]
    fn accessors_do_not_cross_domains_between_render_and_buffer() {
        let rid = RenderProductId::new(1);
        let bid = RuntimeBufferId::new(2);
        let render_p = RuntimeProduct::render(rid);
        let buffer_p = RuntimeProduct::buffer(bid);
        assert!(render_p.as_buffer().is_none());
        assert!(buffer_p.as_render().is_none());
        assert!(render_p.as_value().is_none());
        assert!(buffer_p.as_value().is_none());
    }
}

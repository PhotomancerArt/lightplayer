//! Engine-time product of resolution: direct shader value or render-product handle.

use lps_shared::LpsValueF32;

use crate::render_product::RenderProductId;

/// Payload for produced runtime values: GLSL-compatible data or a render-product handle.
#[derive(Debug, Clone)]
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProductId),
}

impl RuntimeProduct {
    #[must_use]
    pub fn value(value: LpsValueF32) -> Self {
        Self::Value(value)
    }

    #[must_use]
    pub fn render(id: RenderProductId) -> Self {
        Self::Render(id)
    }

    pub fn as_value(&self) -> Option<&LpsValueF32> {
        match self {
            Self::Value(v) => Some(v),
            Self::Render(_) => None,
        }
    }

    pub fn as_render(&self) -> Option<RenderProductId> {
        match self {
            Self::Render(id) => Some(*id),
            Self::Value(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use lps_shared::LpsValueF32;

    use super::RuntimeProduct;
    use crate::render_product::RenderProductId;

    #[test]
    fn runtime_product_value_helper_returns_value() {
        let p = RuntimeProduct::value(LpsValueF32::F32(3.14));
        assert!(matches!(p.as_value(), Some(LpsValueF32::F32(_))));
        assert!(p.as_render().is_none());
    }

    #[test]
    fn runtime_product_render_helper_returns_id() {
        let id = RenderProductId::new(7);
        let p = RuntimeProduct::render(id);
        assert_eq!(p.as_render(), Some(id));
        assert!(p.as_value().is_none());
    }
}

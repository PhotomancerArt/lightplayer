//! Resolver — same-frame demand-resolution cache owner.

use crate::resolver::resolve_error::ResolveError;
use crate::resolver::resolver_cache::ResolverCache;
use crate::runtime_product::RuntimeProduct;
use lpc_model::Revision;
use lpc_model::WithRevision;
use lps_shared::LpsValueF32;

/// Owns the same-frame [`ResolverCache`] for engine demand resolution.
#[derive(Clone, Debug, Default)]
pub struct Resolver {
    cache: ResolverCache,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            cache: ResolverCache::new(),
        }
    }

    pub fn cache(&self) -> &ResolverCache {
        &self.cache
    }

    pub fn cache_mut(&mut self) -> &mut ResolverCache {
        &mut self.cache
    }

    pub fn clear_frame_cache(&mut self) {
        self.cache.clear();
    }
}

/// Materialize a direct [`lpc_model::LpValue`] literal to a versioned runtime
/// product. Shader-compatible values stay in the compact shader ABI shape;
/// richer editor/domain values remain model values.
pub(crate) fn materialize_literal_product(
    value: &lpc_model::LpValue,
    frame: Revision,
) -> WithRevision<RuntimeProduct> {
    let product = match value {
        lpc_model::LpValue::RenderProduct(product) => RuntimeProduct::render(*product),
        other => match model_value_to_lps_value_f32(other) {
            Ok(value) => RuntimeProduct::Value(value),
            Err(_) => RuntimeProduct::model_value(other.clone()),
        },
    };
    WithRevision::new(frame, product)
}

/// Convert [`lpc_model::LpValue`] to [`LpsValueF32`] for shader-compatible literals.
///
/// Not every future runtime domain maps 1:1 into `LpsValueF32`; engine demand
/// resolution may represent other domains as [`RuntimeProduct`](crate::runtime_product::RuntimeProduct).
pub(crate) fn model_value_to_lps_value_f32(
    value: &lpc_model::LpValue,
) -> Result<LpsValueF32, ResolveError> {
    use lpc_model::LpValue;

    match value {
        LpValue::I32(v) => Ok(LpsValueF32::I32(*v)),
        LpValue::U32(v) => Ok(LpsValueF32::U32(*v)),
        LpValue::F32(v) => Ok(LpsValueF32::F32(*v)),
        LpValue::Bool(v) => Ok(LpsValueF32::Bool(*v)),
        LpValue::Vec2(v) => Ok(LpsValueF32::Vec2(*v)),
        LpValue::Vec3(v) => Ok(LpsValueF32::Vec3(*v)),
        LpValue::Vec4(v) => Ok(LpsValueF32::Vec4(*v)),
        LpValue::IVec2(v) => Ok(LpsValueF32::IVec2(*v)),
        LpValue::IVec3(v) => Ok(LpsValueF32::IVec3(*v)),
        LpValue::IVec4(v) => Ok(LpsValueF32::IVec4(*v)),
        LpValue::UVec2(v) => Ok(LpsValueF32::UVec2(*v)),
        LpValue::UVec3(v) => Ok(LpsValueF32::UVec3(*v)),
        LpValue::UVec4(v) => Ok(LpsValueF32::UVec4(*v)),
        LpValue::BVec2(v) => Ok(LpsValueF32::BVec2(*v)),
        LpValue::BVec3(v) => Ok(LpsValueF32::BVec3(*v)),
        LpValue::BVec4(v) => Ok(LpsValueF32::BVec4(*v)),
        LpValue::Mat2x2(v) => Ok(LpsValueF32::Mat2x2(*v)),
        LpValue::Mat3x3(v) => Ok(LpsValueF32::Mat3x3(*v)),
        LpValue::Mat4x4(v) => Ok(LpsValueF32::Mat4x4(*v)),
        LpValue::Array(items) => {
            let mut result = alloc::vec::Vec::with_capacity(items.len());
            for item in items.iter() {
                result.push(model_value_to_lps_value_f32(item)?);
            }
            Ok(LpsValueF32::Array(result.into_boxed_slice()))
        }
        LpValue::Struct { name, fields } => {
            let mut result_fields = alloc::vec::Vec::with_capacity(fields.len());
            for (k, v) in fields.iter() {
                result_fields.push((k.clone(), model_value_to_lps_value_f32(v)?));
            }
            Ok(LpsValueF32::Struct {
                name: name.clone(),
                fields: result_fields,
            })
        }
        LpValue::String(_) | LpValue::Resource(_) | LpValue::RenderProduct(_) => {
            Err(ResolveError::new(alloc::format!(
                "model value cannot be resolved as shader value: {value:?}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_model::LpValue;

    #[test]
    fn model_value_conversion_f32() {
        let val = LpValue::F32(3.14);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        assert!(matches!(lps, LpsValueF32::F32(3.14)));
    }

    #[test]
    fn model_value_conversion_vec3() {
        let val = LpValue::Vec3([1.0, 2.0, 3.0]);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        assert!(matches!(lps, LpsValueF32::Vec3([1.0, 2.0, 3.0])));
    }

    #[test]
    fn model_value_conversion_array() {
        let val = LpValue::Array(alloc::vec![LpValue::F32(1.0), LpValue::F32(2.0),]);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        match lps {
            LpsValueF32::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0], LpsValueF32::F32(1.0)));
                assert!(matches!(items[1], LpsValueF32::F32(2.0)));
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn model_value_conversion_struct() {
        let val = LpValue::Struct {
            name: Some(String::from("Test")),
            fields: alloc::vec![
                (String::from("x"), LpValue::F32(1.0)),
                (String::from("y"), LpValue::F32(2.0)),
            ],
        };
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        match lps {
            LpsValueF32::Struct { name, fields } => {
                assert_eq!(name.as_deref(), Some("Test"));
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }
}

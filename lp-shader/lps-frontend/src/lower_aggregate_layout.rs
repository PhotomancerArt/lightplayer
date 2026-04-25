//! Single funnel from Naga types → `lps_shared::layout` (std430).
//!
//! All aggregate slot allocation, sret-arg sizing, and host marshalling buffer
//! sizing in this crate should use [`aggregate_size_and_align`] and
//! [`array_element_stride`]. This keeps LPIR slot bytes aligned with
//! `lps_shared::layout` by construction.
//!
//! Naga handle → [`lps_shared::LpsType`] reuses [`crate::naga_types::naga_type_handle_to_lps`]
//! so the layout view stays consistent with signature / metadata lowering.

use alloc::format;

use naga::{Handle, Module, Type};

use lps_shared::layout::{array_stride, type_alignment, type_size};
use lps_shared::{LayoutRules, LpsType};

use crate::lower_error::LowerError;
use crate::naga_types;

/// `(size_bytes, align_bytes)` for `naga_ty` under std430.
pub(crate) fn aggregate_size_and_align(
    module: &Module,
    naga_ty: Handle<Type>,
) -> Result<(u32, u32), LowerError> {
    let lps = naga_to_lps_type(module, naga_ty)?;
    let size = type_size(&lps, LayoutRules::Std430);
    let align = type_alignment(&lps, LayoutRules::Std430);
    Ok((size as u32, align as u32))
}

/// Element stride for an array of `element_naga_ty` under std430.
pub(crate) fn array_element_stride(
    module: &Module,
    element_naga_ty: Handle<Type>,
) -> Result<u32, LowerError> {
    let lps = naga_to_lps_type(module, element_naga_ty)?;
    Ok(array_stride(&lps, LayoutRules::Std430) as u32)
}

/// Convert a Naga type handle to the [`LpsType`] used by `lps_shared::layout`.
pub(crate) fn naga_to_lps_type(
    module: &Module,
    handle: Handle<Type>,
) -> Result<LpsType, LowerError> {
    naga_types::naga_type_handle_to_lps(module, handle)
        .map_err(|e| LowerError::UnsupportedType(format!("lower_aggregate_layout: {e}")))
}

#[cfg(test)]
mod tests {
    use naga::Handle;

    use super::*;
    use crate::compile;
    use lps_shared::layout::{type_alignment, type_size};

    const R: LayoutRules = LayoutRules::Std430;

    fn first_ty_matching(
        module: &naga::Module,
        mut pred: impl FnMut(&naga::Type) -> bool,
    ) -> Handle<Type> {
        for (h, t) in module.types.iter() {
            if pred(t) {
                return h;
            }
        }
        panic!("no matching type");
    }

    #[test]
    fn std430_float4_array() {
        let naga = compile("void f() { float a[4]; }").unwrap();
        let h = first_ty_matching(&naga.module, |t| {
            matches!(&t.inner, naga::TypeInner::Array { .. })
        });
        let (sz, al) = aggregate_size_and_align(&naga.module, h).unwrap();
        assert_eq!((sz, al), (16, 4));
    }

    #[test]
    fn std430_vec2_array_3() {
        let naga = compile("void f() { vec2 a[3]; }").unwrap();
        let h = first_ty_matching(&naga.module, |t| {
            matches!(&t.inner, naga::TypeInner::Array { .. })
        });
        let (sz, al) = aggregate_size_and_align(&naga.module, h).unwrap();
        assert_eq!((sz, al), (24, 8));
    }

    #[test]
    fn std430_vec3_array_3() {
        let naga = compile("void f() { vec3 a[3]; }").unwrap();
        let h = first_ty_matching(&naga.module, |t| {
            matches!(&t.inner, naga::TypeInner::Array { .. })
        });
        let (sz, al) = aggregate_size_and_align(&naga.module, h).unwrap();
        // vec3 stride 12 in std430 (not Naga’s potential 16B array stride).
        assert_eq!((sz, al), (36, 4));
    }

    #[test]
    fn std430_vec4_array_2() {
        let naga = compile("void f() { vec4 a[2]; }").unwrap();
        let h = first_ty_matching(&naga.module, |t| {
            matches!(&t.inner, naga::TypeInner::Array { .. })
        });
        let (sz, al) = aggregate_size_and_align(&naga.module, h).unwrap();
        assert_eq!((sz, al), (32, 16));
    }

    #[test]
    fn std430_bvec4_array_2() {
        let naga = compile("void f() { bvec4 a[2]; }").unwrap();
        let h = first_ty_matching(&naga.module, |t| {
            matches!(&t.inner, naga::TypeInner::Array { .. })
        });
        let (sz, al) = aggregate_size_and_align(&naga.module, h).unwrap();
        assert_eq!((sz, al), (32, 16));
    }

    #[test]
    fn std430_mat3_return() {
        let naga = compile("mat3 f() { return mat3(1.0); }").unwrap();
        let f = naga
            .functions
            .iter()
            .find(|(_, i)| i.name == "f")
            .expect("f")
            .0;
        let ret = naga.module.functions[f].result.as_ref().expect("return").ty;
        let (sz, al) = aggregate_size_and_align(&naga.module, ret).unwrap();
        assert_eq!((sz, al), (36, 4));
    }

    #[test]
    fn std430_mat4_return() {
        let naga = compile("mat4 f() { return mat4(1.0); }").unwrap();
        let f = naga
            .functions
            .iter()
            .find(|(_, i)| i.name == "f")
            .expect("f")
            .0;
        let ret = naga.module.functions[f].result.as_ref().expect("return").ty;
        let (sz, al) = aggregate_size_and_align(&naga.module, ret).unwrap();
        assert_eq!((sz, al), (64, 16));
    }

    #[test]
    fn std430_struct_vec3_float() {
        let naga = compile("struct S { vec3 v; float f; }; void g() { S s; }").unwrap();
        let h = first_ty_matching(&naga.module, |t| {
            matches!(&t.inner, naga::TypeInner::Struct { .. })
        });
        let (sz, al) = aggregate_size_and_align(&naga.module, h).unwrap();
        assert_eq!((sz, al), (16, 4));
    }

    #[test]
    fn hand_built_lps_lock_std430_numbers() {
        use alloc::boxed::Box;

        // Cross-check: same sizes as the Naga-driven cases above.
        let t4 = LpsType::Array {
            element: Box::new(LpsType::Float),
            len: 4,
        };
        assert_eq!(type_size(&t4, R), 16);
        assert_eq!(type_alignment(&t4, R), 4);

        let v3_3 = LpsType::Array {
            element: Box::new(LpsType::Vec3),
            len: 3,
        };
        assert_eq!(type_size(&v3_3, R), 36);
        assert_eq!(type_alignment(&v3_3, R), 4);
    }
}

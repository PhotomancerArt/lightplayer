//! Tests for LPFX attribute parsing

use lp_builtin_gen::lpfx::errors::Variant;
use lp_builtin_gen::lpfx::parse::parse_lpfx_attribute;
use syn::parse_quote;

#[test]
fn test_parse_non_decimal_attribute() {
    let attr: syn::Attribute = parse_quote! {
        #[lpfx_impl("u32 lpfx_hash1(u32 x, u32 seed)")]
    };

    let result = parse_lpfx_attribute(&attr, "test_func", "test.rs").unwrap();

    assert_eq!(result.variant, None);
    assert_eq!(result.glsl_signature, "u32 lpfx_hash1(u32 x, u32 seed)");
}

#[test]
fn test_parse_decimal_f32_attribute() {
    let attr: syn::Attribute = parse_quote! {
        #[lpfx_impl(f32, "float lpfx_snoise3(vec3 p, u32 seed)")]
    };

    let result = parse_lpfx_attribute(&attr, "test_func", "test.rs").unwrap();

    assert_eq!(result.variant, Some(Variant::F32));
    assert_eq!(
        result.glsl_signature,
        "float lpfx_snoise3(vec3 p, u32 seed)"
    );
}

#[test]
fn test_parse_decimal_q32_attribute() {
    let attr: syn::Attribute = parse_quote! {
        #[lpfx_impl(q32, "float lpfx_snoise3(vec3 p, u32 seed)")]
    };

    let result = parse_lpfx_attribute(&attr, "test_func", "test.rs").unwrap();

    assert_eq!(result.variant, Some(Variant::Q32));
    assert_eq!(
        result.glsl_signature,
        "float lpfx_snoise3(vec3 p, u32 seed)"
    );
}

#[test]
fn test_parse_invalid_attribute_missing_args() {
    let attr: syn::Attribute = parse_quote! {
        #[lpfx_impl]
    };

    let result = parse_lpfx_attribute(&attr, "test_func", "test.rs");
    assert!(result.is_err());
}

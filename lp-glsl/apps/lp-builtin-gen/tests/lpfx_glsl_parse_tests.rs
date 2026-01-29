//! Tests for GLSL signature parsing

use lp_builtin_gen::lpfx::glsl_parse::parse_glsl_signature;

#[test]
fn test_parse_simple_signature() {
    // Note: GLSL uses 'uint' not 'u32' in source code
    let sig = "uint lpfx_hash1(uint x, uint seed)";
    let result = parse_glsl_signature(sig, "test_func", "test.rs");
    if let Err(e) = &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_ok(), "Failed to parse signature: {:?}", result);

    let func_sig = result.unwrap();
    assert_eq!(func_sig.name, "lpfx_hash1");
    assert_eq!(func_sig.parameters.len(), 2);
}

#[test]
fn test_parse_vector_signature() {
    // Note: GLSL uses 'uint' not 'u32' in source code
    let sig = "float lpfx_snoise3(vec3 p, uint seed)";
    let result = parse_glsl_signature(sig, "test_func", "test.rs");
    assert!(result.is_ok(), "Failed to parse signature: {:?}", result);

    let func_sig = result.unwrap();
    assert_eq!(func_sig.name, "lpfx_snoise3");
    assert_eq!(func_sig.parameters.len(), 2);
}

#[test]
fn test_parse_invalid_signature() {
    let sig = "invalid syntax here";
    let result = parse_glsl_signature(sig, "test_func", "test.rs");
    assert!(result.is_err());
}

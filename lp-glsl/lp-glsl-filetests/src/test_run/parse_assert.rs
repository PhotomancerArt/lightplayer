//! Parse assertions, compare values.

use crate::parse::test_type::ComparisonOp;
use anyhow::Result;
use lp_glsl_abi::GlslValue;

/// Bases for `type[size](...)` array constructors (longer names first).
const TYPED_ARRAY_BASES: &[&str] = &[
    "ivec4", "uvec4", "bvec4", "ivec3", "uvec3", "bvec3", "ivec2", "uvec2", "bvec2", "vec4",
    "vec3", "vec2", "uint", "bool", "int", "float", "mat4", "mat3", "mat2",
];

fn parse_typed_array_prefix(s: &str) -> Option<(&str, &str)> {
    for base in TYPED_ARRAY_BASES {
        if let Some(rest) = s.strip_prefix(base) {
            if rest.starts_with('[') {
                return Some((*base, rest));
            }
        }
    }
    None
}

/// Content after `(`, find matching `)` and return inner slice (not including parens).
fn paren_contents(s: &str) -> Result<&str> {
    let mut depth = 1usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&s[..i]);
                }
            }
            _ => {}
        }
    }
    Err(anyhow::anyhow!("unclosed '(' in constructor"))
}

/// Parse `[N](...)` after the type name; `rest` begins with `[`.
fn parse_array_size_and_constructor_args(rest: &str) -> Result<(usize, &str)> {
    let rest = rest
        .strip_prefix('[')
        .ok_or_else(|| anyhow::anyhow!("internal: expected '['"))?;
    let rb = rest
        .find(']')
        .ok_or_else(|| anyhow::anyhow!("unclosed '[' in array type"))?;
    let size: usize = rest[..rb]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid array size"))?;
    let after = rest[rb + 1..].trim_start();
    let after_paren = after
        .strip_prefix('(')
        .ok_or_else(|| anyhow::anyhow!("expected '(' after array size"))?;
    let inner = paren_contents(after_paren)?;
    Ok((size, inner))
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                let t = s[start..i].trim();
                if !t.is_empty() {
                    parts.push(t);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let t = s[start..].trim();
    if !t.is_empty() {
        parts.push(t);
    }
    parts
}

fn array_element_matches_base(base: &str, v: &GlslValue) -> bool {
    match base {
        "float" => matches!(v, GlslValue::F32(_)),
        "int" => matches!(v, GlslValue::I32(_)),
        "uint" => matches!(v, GlslValue::U32(_)),
        "bool" => matches!(v, GlslValue::Bool(_)),
        "vec2" => matches!(v, GlslValue::Vec2(_)),
        "vec3" => matches!(v, GlslValue::Vec3(_)),
        "vec4" => matches!(v, GlslValue::Vec4(_)),
        "ivec2" => matches!(v, GlslValue::IVec2(_)),
        "ivec3" => matches!(v, GlslValue::IVec3(_)),
        "ivec4" => matches!(v, GlslValue::IVec4(_)),
        "uvec2" => matches!(v, GlslValue::UVec2(_)),
        "uvec3" => matches!(v, GlslValue::UVec3(_)),
        "uvec4" => matches!(v, GlslValue::UVec4(_)),
        "bvec2" => matches!(v, GlslValue::BVec2(_)),
        "bvec3" => matches!(v, GlslValue::BVec3(_)),
        "bvec4" => matches!(v, GlslValue::BVec4(_)),
        "mat2" => matches!(v, GlslValue::Mat2x2(_)),
        "mat3" => matches!(v, GlslValue::Mat3x3(_)),
        "mat4" => matches!(v, GlslValue::Mat4x4(_)),
        _ => false,
    }
}

/// If `s` is a `type[N](...)` array constructor, parse and return `Some(Ok(value))`.
/// Returns `Ok(None)` if the string does not start with a known array type prefix.
fn parse_typed_array_constructor(s: &str) -> Result<Option<GlslValue>> {
    let s = s.trim();
    let Some((base, rest)) = parse_typed_array_prefix(s) else {
        return Ok(None);
    };
    let (size, inner) = parse_array_size_and_constructor_args(rest)?;
    let parts = split_top_level_commas(inner);
    if parts.len() != size {
        return Err(anyhow::anyhow!(
            "array constructor expects {} elements, got {}",
            size,
            parts.len()
        ));
    }
    let mut elems = Vec::with_capacity(size);
    for p in parts {
        let v = parse_glsl_value(p)?;
        if !array_element_matches_base(base, &v) {
            return Err(anyhow::anyhow!(
                "array element type mismatch for base `{base}`: got {v:?}"
            ));
        }
        elems.push(v);
    }
    Ok(Some(GlslValue::Array(elems.into_boxed_slice())))
}

/// Parse a GLSL value from a string.
/// Supports scalars, vectors, and matrices.
pub fn parse_glsl_value(s: &str) -> Result<GlslValue> {
    let s = s.trim();

    // Check for uint suffix (u or U)
    if s.ends_with('u') || s.ends_with('U') {
        let num_str = &s[..s.len() - 1];
        if let Ok(u) = num_str.parse::<u32>() {
            return Ok(GlslValue::U32(u));
        }
    }

    // Try parsing as integer
    if let Ok(i) = s.parse::<i32>() {
        return Ok(GlslValue::I32(i));
    }

    // Try parsing as float
    if let Ok(f) = s.parse::<f32>() {
        return Ok(GlslValue::F32(f));
    }

    // Try parsing as boolean
    match s {
        "true" => return Ok(GlslValue::Bool(true)),
        "false" => return Ok(GlslValue::Bool(false)),
        _ => {}
    }

    // Typed array constructors: float[3](1.0, 2.0, 3.0), vec2[2](...)
    if let Some(v) = parse_typed_array_constructor(s)? {
        return Ok(v);
    }

    // Try parsing as vector or matrix constructor using GlslValue::parse
    // This uses the GLSL parser to handle constructors like vec2(1.0, 2.0)
    if let Ok(value) = GlslValue::parse(s) {
        return Ok(value);
    }

    anyhow::bail!("failed to parse GLSL value: {s}")
}

/// Parse a function call expression (e.g., "add_float(1.5, 2.5)") into function name and arguments.
/// Returns (function_name, argument_strings).
pub fn parse_function_call(expression: &str) -> Result<(String, Vec<String>)> {
    let expression = expression.trim();

    // Find the opening parenthesis
    let open_paren = expression
        .find('(')
        .ok_or_else(|| anyhow::anyhow!("function call must contain '(': {expression}"))?;

    // Extract function name (everything before the opening parenthesis)
    let func_name = expression[..open_paren].trim().to_string();
    if func_name.is_empty() {
        return Err(anyhow::anyhow!(
            "function name is empty in expression: {expression}"
        ));
    }

    // Find the matching closing parenthesis
    let args_str = &expression[open_paren + 1..];
    let mut paren_depth = 1;
    let mut close_paren_pos = None;

    for (i, ch) in args_str.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth == 0 {
                    close_paren_pos = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let close_paren_pos = close_paren_pos
        .ok_or_else(|| anyhow::anyhow!("unmatched parentheses in expression: {expression}"))?;

    // Extract arguments string (between parentheses)
    let args_str = &args_str[..close_paren_pos];

    // Parse arguments (split by comma, respecting nested parentheses)
    let mut args = Vec::new();
    if !args_str.trim().is_empty() {
        let mut current_arg = String::new();
        let mut paren_depth = 0;

        for ch in args_str.chars() {
            match ch {
                '(' => {
                    paren_depth += 1;
                    current_arg.push(ch);
                }
                ')' => {
                    paren_depth -= 1;
                    current_arg.push(ch);
                }
                ',' => {
                    if paren_depth == 0 {
                        // This comma is at the top level, split here
                        args.push(current_arg.trim().to_string());
                        current_arg.clear();
                    } else {
                        // This comma is inside nested parentheses, keep it
                        current_arg.push(ch);
                    }
                }
                _ => current_arg.push(ch),
            }
        }

        // Add the last argument
        if !current_arg.trim().is_empty() {
            args.push(current_arg.trim().to_string());
        }
    }

    Ok((func_name, args))
}

/// Parse function call arguments from strings to GlslValue.
pub fn parse_function_arguments(arg_strings: &[String]) -> Result<Vec<GlslValue>> {
    arg_strings
        .iter()
        .map(|arg_str| parse_glsl_value(arg_str))
        .collect()
}

/// Format a GLSL value as a string (temporary stub - will be moved to util::file_update).
fn format_glsl_value(value: &GlslValue) -> String {
    // TODO: Move this to util::file_update in Phase 4
    // For now, use a simple format
    format!("{value:?}")
}

/// Compare actual and expected values.
pub fn compare_results(
    actual: &GlslValue,
    expected: &GlslValue,
    comparison: ComparisonOp,
    tolerance: Option<f32>,
) -> Result<(), String> {
    match comparison {
        ComparisonOp::Exact => {
            if actual.eq(expected) {
                Ok(())
            } else {
                Err(format!(
                    "expected {}, got {}",
                    format_glsl_value(expected),
                    format_glsl_value(actual)
                ))
            }
        }
        ComparisonOp::Approx => {
            let tolerance = tolerance.unwrap_or(GlslValue::DEFAULT_TOLERANCE);
            if actual.approx_eq(expected, tolerance) {
                Ok(())
            } else {
                Err(format!(
                    "expected {} (tolerance: {}), got {}",
                    format_glsl_value(expected),
                    tolerance,
                    format_glsl_value(actual)
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::test_type::ComparisonOp;
    use lp_glsl_abi::GlslValue;

    #[test]
    fn test_parse_glsl_value_int() {
        assert!(parse_glsl_value("42").unwrap().eq(&GlslValue::I32(42)));
        assert!(parse_glsl_value("-10").unwrap().eq(&GlslValue::I32(-10)));
    }

    #[test]
    fn test_parse_glsl_value_float() {
        let v1 = parse_glsl_value("3.14").unwrap();
        assert!(v1.approx_eq(&GlslValue::F32(3.14), 0.001));
        let v2 = parse_glsl_value("-0.5").unwrap();
        assert!(v2.approx_eq(&GlslValue::F32(-0.5), 0.001));
    }

    #[test]
    fn test_parse_glsl_value_uint() {
        assert!(parse_glsl_value("42u").unwrap().eq(&GlslValue::U32(42)));
        assert!(parse_glsl_value("100U").unwrap().eq(&GlslValue::U32(100)));
    }

    #[test]
    fn test_parse_glsl_value_bool() {
        assert!(parse_glsl_value("true").unwrap().eq(&GlslValue::Bool(true)));
        assert!(
            parse_glsl_value("false")
                .unwrap()
                .eq(&GlslValue::Bool(false))
        );
    }

    #[test]
    fn test_parse_function_call_simple() {
        let (name, args) = parse_function_call("add(1, 2)").unwrap();
        assert_eq!(name, "add");
        assert_eq!(args, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn test_parse_function_call_nested() {
        let (name, args) = parse_function_call("test(vec2(1.0, 2.0), 3)").unwrap();
        assert_eq!(name, "test");
        assert_eq!(args, vec!["vec2(1.0, 2.0)".to_string(), "3".to_string()]);
    }

    #[test]
    fn test_parse_function_call_no_args() {
        let (name, args) = parse_function_call("test()").unwrap();
        assert_eq!(name, "test");
        assert_eq!(args, Vec::<String>::new());
    }

    #[test]
    fn test_parse_function_call_invalid() {
        assert!(parse_function_call("test").is_err());
        assert!(parse_function_call("test(").is_err());
        assert!(parse_function_call("").is_err());
    }

    #[test]
    fn test_compare_results_exact_match() {
        let actual = GlslValue::I32(42);
        let expected = GlslValue::I32(42);
        assert!(compare_results(&actual, &expected, ComparisonOp::Exact, None).is_ok());
    }

    #[test]
    fn test_compare_results_exact_mismatch() {
        let actual = GlslValue::I32(42);
        let expected = GlslValue::I32(43);
        assert!(compare_results(&actual, &expected, ComparisonOp::Exact, None).is_err());
    }

    #[test]
    fn test_compare_results_approx_match() {
        let actual = GlslValue::F32(1.0);
        let expected = GlslValue::F32(1.0001);
        assert!(compare_results(&actual, &expected, ComparisonOp::Approx, Some(0.001)).is_ok());
    }

    #[test]
    fn test_compare_results_approx_mismatch() {
        let actual = GlslValue::F32(1.0);
        let expected = GlslValue::F32(1.1);
        assert!(compare_results(&actual, &expected, ComparisonOp::Approx, Some(0.001)).is_err());
    }
}

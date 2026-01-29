//! GLSL value types for function arguments and return values

use crate::error::{ErrorCode, GlslError};
use glsl::syntax::{Expr, JumpStatement, SimpleStatement, Statement};

use alloc::{format, vec::Vec};

/// Truncate f32 toward zero (no_std compatible)
/// Casts to i32 which truncates toward zero, then to i64
#[inline]
fn trunc_f32(f: f32) -> i64 {
    (f as i32) as i64
}

/// GLSL value types for function arguments
///
/// ## Matrix Storage Format
///
/// Matrices are stored in **column-major order** per GLSL specification.
/// The internal representation uses `m[col][row]` indexing, matching GLSL semantics.
///
/// Example: `mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))`
/// - Column 0: [1.0, 2.0]
/// - Column 1: [3.0, 4.0]
/// - Storage (column-major): [1.0, 2.0, 3.0, 4.0]
/// - Internal representation: `[[1.0, 2.0], [3.0, 4.0]]` (m[col][row])
///   - m[0][0] = 1.0 (col 0, row 0)
///   - m[0][1] = 2.0 (col 0, row 1)
///   - m[1][0] = 3.0 (col 1, row 0)
///   - m[1][1] = 4.0 (col 1, row 1)
///
/// To access column `col`, use `m[col][row]` for `row` in 0..rows.
/// To access row `row`, use `m[col][row]` for `col` in 0..cols.
#[derive(Debug, Clone)]
pub enum GlslValue {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]), // [[col0_row0, col0_row1], [col1_row0, col1_row1]]
    Mat3x3([[f32; 3]; 3]), // [[col0_row0, col0_row1, col0_row2], [col1_row0, ...], ...]
    Mat4x4([[f32; 4]; 4]), // [[col0_row0, col0_row1, col0_row2, col0_row3], [col1_row0, ...], ...]
}

impl GlslValue {
    /// Parse a literal value string into GlslValue using GLSL parser
    /// Supports literals: integers, floats, booleans, vectors, and matrices
    /// Uses type checking to ensure valid literal syntax
    pub fn parse(literal_str: &str) -> Result<Self, GlslError> {
        // Wrap the literal in a minimal function to parse it
        // We'll try different return types to determine the literal type
        let wrappers = [
            format!("int main() {{ return {literal_str}; }}"),
            format!("uint main() {{ return {literal_str}; }}"),
            format!("float main() {{ return {literal_str}; }}"),
            format!("bool main() {{ return {literal_str}; }}"),
            format!("vec2 main() {{ return {literal_str}; }}"),
            format!("vec3 main() {{ return {literal_str}; }}"),
            format!("vec4 main() {{ return {literal_str}; }}"),
            format!("ivec2 main() {{ return {literal_str}; }}"),
            format!("ivec3 main() {{ return {literal_str}; }}"),
            format!("ivec4 main() {{ return {literal_str}; }}"),
            format!("uvec2 main() {{ return {literal_str}; }}"),
            format!("uvec3 main() {{ return {literal_str}; }}"),
            format!("uvec4 main() {{ return {literal_str}; }}"),
            format!("bvec2 main() {{ return {literal_str}; }}"),
            format!("bvec3 main() {{ return {literal_str}; }}"),
            format!("bvec4 main() {{ return {literal_str}; }}"),
            format!("mat2 main() {{ return {literal_str}; }}"),
            format!("mat3 main() {{ return {literal_str}; }}"),
            format!("mat4 main() {{ return {literal_str}; }}"),
        ];

        for wrapper in &wrappers {
            if let Ok(shader) = glsl::parser::Parse::parse(wrapper) {
                // Extract the return statement expression
                if let Some(expr) = extract_return_expression(&shader) {
                    match expr {
                        Expr::IntConst(n, _) => {
                            return Ok(GlslValue::I32(*n));
                        }
                        Expr::UIntConst(n, _) => {
                            return Ok(GlslValue::U32(*n));
                        }
                        Expr::FloatConst(f, _) => {
                            return Ok(GlslValue::F32(*f));
                        }
                        Expr::BoolConst(b, _) => {
                            return Ok(GlslValue::Bool(*b));
                        }
                        Expr::Unary(op, unary_expr, _) => {
                            // Handle unary minus for negative numbers
                            use glsl::syntax::UnaryOp;
                            if let UnaryOp::Minus = *op {
                                match **unary_expr {
                                    Expr::IntConst(n, _) => {
                                        return Ok(GlslValue::I32(-n));
                                    }
                                    Expr::UIntConst(n, _) => {
                                        // -1u gives 0xffffffffu (wrapping negation: !n + 1)
                                        return Ok(GlslValue::U32((!n).wrapping_add(1)));
                                    }
                                    Expr::FloatConst(f, _) => {
                                        return Ok(GlslValue::F32(-f));
                                    }
                                    _ => {
                                        // Not a negated literal, continue
                                        continue;
                                    }
                                }
                            } else {
                                // Not a minus operator, continue
                                continue;
                            }
                        }
                        Expr::FunCall(func_ident, args, _) => {
                            // Handle vector and matrix constructors
                            if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                                let type_name = ident.name.as_str();
                                match type_name {
                                    "vec2" => {
                                        if let Ok(v) = parse_vector_constructor(args, 2) {
                                            return Ok(GlslValue::Vec2([v[0], v[1]]));
                                        }
                                    }
                                    "vec3" => {
                                        if let Ok(v) = parse_vector_constructor(args, 3) {
                                            return Ok(GlslValue::Vec3([v[0], v[1], v[2]]));
                                        }
                                    }
                                    "vec4" => {
                                        if let Ok(v) = parse_vector_constructor(args, 4) {
                                            return Ok(GlslValue::Vec4([v[0], v[1], v[2], v[3]]));
                                        }
                                    }
                                    "ivec2" => {
                                        if let Ok(v) = parse_int_vector_constructor(args, 2) {
                                            return Ok(GlslValue::IVec2([v[0], v[1]]));
                                        }
                                    }
                                    "ivec3" => {
                                        if let Ok(v) = parse_int_vector_constructor(args, 3) {
                                            return Ok(GlslValue::IVec3([v[0], v[1], v[2]]));
                                        }
                                    }
                                    "ivec4" => {
                                        if let Ok(v) = parse_int_vector_constructor(args, 4) {
                                            return Ok(GlslValue::IVec4([v[0], v[1], v[2], v[3]]));
                                        }
                                    }
                                    "uvec2" => {
                                        if let Ok(v) = parse_uint_vector_constructor(args, 2) {
                                            return Ok(GlslValue::UVec2([v[0], v[1]]));
                                        }
                                    }
                                    "uvec3" => {
                                        if let Ok(v) = parse_uint_vector_constructor(args, 3) {
                                            return Ok(GlslValue::UVec3([v[0], v[1], v[2]]));
                                        }
                                    }
                                    "uvec4" => {
                                        if let Ok(v) = parse_uint_vector_constructor(args, 4) {
                                            return Ok(GlslValue::UVec4([v[0], v[1], v[2], v[3]]));
                                        }
                                    }
                                    "bvec2" => {
                                        if let Ok(v) = parse_bool_vector_constructor(args, 2) {
                                            return Ok(GlslValue::BVec2([v[0], v[1]]));
                                        }
                                    }
                                    "bvec3" => {
                                        if let Ok(v) = parse_bool_vector_constructor(args, 3) {
                                            return Ok(GlslValue::BVec3([v[0], v[1], v[2]]));
                                        }
                                    }
                                    "bvec4" => {
                                        if let Ok(v) = parse_bool_vector_constructor(args, 4) {
                                            return Ok(GlslValue::BVec4([v[0], v[1], v[2], v[3]]));
                                        }
                                    }
                                    "mat2" => {
                                        if let Ok(m) = parse_matrix_constructor(args, 2) {
                                            return Ok(GlslValue::Mat2x2([
                                                [m[0][0], m[0][1]],
                                                [m[1][0], m[1][1]],
                                            ]));
                                        }
                                    }
                                    "mat3" => {
                                        if let Ok(m) = parse_matrix_constructor(args, 3) {
                                            return Ok(GlslValue::Mat3x3([
                                                [m[0][0], m[0][1], m[0][2]],
                                                [m[1][0], m[1][1], m[1][2]],
                                                [m[2][0], m[2][1], m[2][2]],
                                            ]));
                                        }
                                    }
                                    "mat4" => {
                                        if let Ok(m) = parse_matrix_constructor(args, 4) {
                                            return Ok(GlslValue::Mat4x4([
                                                [m[0][0], m[0][1], m[0][2], m[0][3]],
                                                [m[1][0], m[1][1], m[1][2], m[1][3]],
                                                [m[2][0], m[2][1], m[2][2], m[2][3]],
                                                [m[3][0], m[3][1], m[3][2], m[3][3]],
                                            ]));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            continue;
                        }
                        _ => {
                            // Not a literal or constructor, continue to next wrapper
                            continue;
                        }
                    }
                }
            }
        }

        Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "invalid literal: `{literal_str}` (must be an integer, float, boolean, vector, or matrix literal)"
            ),
        ))
    }

    /// Exact equality comparison (==)
    /// For integers and booleans: exact match required
    /// For floats: exact match required (use `approx_eq` for tolerance-based comparison)
    /// For vectors/matrices: exact match for all components
    pub fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (GlslValue::I32(a), GlslValue::I32(b)) => a == b,
            (GlslValue::U32(a), GlslValue::U32(b)) => a == b,
            (GlslValue::F32(a), GlslValue::F32(b)) => a == b, // Exact equality
            (GlslValue::Bool(a), GlslValue::Bool(b)) => a == b,
            (GlslValue::Vec2(a), GlslValue::Vec2(b)) => a == b,
            (GlslValue::Vec3(a), GlslValue::Vec3(b)) => a == b,
            (GlslValue::Vec4(a), GlslValue::Vec4(b)) => a == b,
            (GlslValue::IVec2(a), GlslValue::IVec2(b)) => a == b,
            (GlslValue::IVec3(a), GlslValue::IVec3(b)) => a == b,
            (GlslValue::IVec4(a), GlslValue::IVec4(b)) => a == b,
            (GlslValue::UVec2(a), GlslValue::UVec2(b)) => a == b,
            (GlslValue::UVec3(a), GlslValue::UVec3(b)) => a == b,
            (GlslValue::UVec4(a), GlslValue::UVec4(b)) => a == b,
            (GlslValue::BVec2(a), GlslValue::BVec2(b)) => a == b,
            (GlslValue::BVec3(a), GlslValue::BVec3(b)) => a == b,
            (GlslValue::BVec4(a), GlslValue::BVec4(b)) => a == b,
            (GlslValue::Mat2x2(a), GlslValue::Mat2x2(b)) => a == b,
            (GlslValue::Mat3x3(a), GlslValue::Mat3x3(b)) => a == b,
            (GlslValue::Mat4x4(a), GlslValue::Mat4x4(b)) => a == b,
            _ => false, // Type mismatch
        }
    }

    /// Approximate equality comparison (~=) with tolerance
    /// For floats: checks if values are within tolerance
    /// For integers and booleans: falls back to exact equality
    /// For vectors/matrices: checks each component within tolerance
    pub fn approx_eq(&self, other: &Self, tolerance: f32) -> bool {
        match (self, other) {
            (GlslValue::I32(a), GlslValue::I32(b)) => a == b, // Exact for ints
            (GlslValue::U32(a), GlslValue::U32(b)) => a == b, // Exact for uints
            (GlslValue::F32(a), GlslValue::F32(b)) => (a - b).abs() <= tolerance,
            (GlslValue::Bool(a), GlslValue::Bool(b)) => a == b, // Exact for bools
            (GlslValue::Vec2(a), GlslValue::Vec2(b)) => a
                .iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (GlslValue::Vec3(a), GlslValue::Vec3(b)) => a
                .iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (GlslValue::Vec4(a), GlslValue::Vec4(b)) => a
                .iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (GlslValue::IVec2(a), GlslValue::IVec2(b)) => a == b, // Exact for ints
            (GlslValue::IVec3(a), GlslValue::IVec3(b)) => a == b, // Exact for ints
            (GlslValue::IVec4(a), GlslValue::IVec4(b)) => a == b, // Exact for ints
            (GlslValue::UVec2(a), GlslValue::UVec2(b)) => a == b, // Exact for uints
            (GlslValue::UVec3(a), GlslValue::UVec3(b)) => a == b, // Exact for uints
            (GlslValue::UVec4(a), GlslValue::UVec4(b)) => a == b, // Exact for uints
            (GlslValue::BVec2(a), GlslValue::BVec2(b)) => a == b, // Exact for bools
            (GlslValue::BVec3(a), GlslValue::BVec3(b)) => a == b, // Exact for bools
            (GlslValue::BVec4(a), GlslValue::BVec4(b)) => a == b, // Exact for bools
            (GlslValue::Mat2x2(a), GlslValue::Mat2x2(b)) => a
                .iter()
                .flatten()
                .zip(b.iter().flatten())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (GlslValue::Mat3x3(a), GlslValue::Mat3x3(b)) => a
                .iter()
                .flatten()
                .zip(b.iter().flatten())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            (GlslValue::Mat4x4(a), GlslValue::Mat4x4(b)) => a
                .iter()
                .flatten()
                .zip(b.iter().flatten())
                .all(|(x, y)| (x - y).abs() <= tolerance),
            _ => false, // Type mismatch
        }
    }

    /// Default tolerance for float comparisons (1e-4)
    pub const DEFAULT_TOLERANCE: f32 = 1e-4;

    /// Approximate equality with default tolerance
    pub fn approx_eq_default(&self, other: &Self) -> bool {
        self.approx_eq(other, Self::DEFAULT_TOLERANCE)
    }
}

/// Extract the return expression from a parsed shader
/// Assumes the shader has a single function with a single return statement
fn extract_return_expression(shader: &glsl::syntax::TranslationUnit) -> Option<&Expr> {
    for decl in &shader.0 {
        if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
            // The function body is a CompoundStatement with a statement_list
            for stmt in &func.statement.statement_list {
                if let Statement::Simple(simple_stmt) = stmt {
                    if let SimpleStatement::Jump(JumpStatement::Return(Some(ref expr))) =
                        **simple_stmt
                    {
                        return Some(expr);
                    }
                }
            }
        }
    }
    None
}

/// Parse a vector constructor expression into a Vec of floats
/// Returns a Vec that can be converted to the appropriate array size
fn parse_vector_constructor(args: &[Expr], dim: usize) -> Result<Vec<f32>, GlslError> {
    let mut components = Vec::new();

    for arg in args {
        match arg {
            Expr::FloatConst(f, _) => components.push(*f),
            Expr::IntConst(n, _) => components.push(*n as f32),
            Expr::FunCall(func_ident, args, _) => {
                // Handle nested vector constructors (e.g., vec2(1.0, 2.0))
                if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                    let type_name = ident.name.as_str();
                    if type_name.starts_with("vec") || type_name.starts_with("ivec") {
                        let nested_dim = match type_name {
                            "vec2" | "ivec2" => 2,
                            "vec3" | "ivec3" => 3,
                            "vec4" | "ivec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_vector_constructor(args, nested_dim)?;
                        components.extend_from_slice(&nested);
                    }
                }
            }
            Expr::Unary(op, unary_expr, _) => {
                use glsl::syntax::UnaryOp;
                if let UnaryOp::Minus = *op {
                    match **unary_expr {
                        Expr::FloatConst(f, _) => components.push(-f),
                        Expr::IntConst(n, _) => components.push(-(n as f32)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "invalid vector constructor argument",
                            ));
                        }
                    }
                } else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "invalid vector constructor argument",
                    ));
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "invalid vector constructor argument",
                ));
            }
        }
    }

    if components.len() != dim {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "vector constructor expects {} components, got {}",
                dim,
                components.len()
            ),
        ));
    }

    Ok(components)
}

/// Parse a boolean vector constructor expression into a Vec of bools
/// Returns a Vec that can be converted to the appropriate array size
fn parse_bool_vector_constructor(args: &[Expr], dim: usize) -> Result<Vec<bool>, GlslError> {
    let mut components = Vec::new();

    for arg in args {
        match arg {
            Expr::BoolConst(b, _) => components.push(*b),
            Expr::IntConst(n, _) => {
                // Convert int to bool: 0 → false, non-zero → true
                components.push(*n != 0);
            }
            Expr::FloatConst(f, _) => {
                // Convert float to bool: 0.0 → false, non-zero → true
                components.push(*f != 0.0);
            }
            Expr::FunCall(func_ident, args, _) => {
                // Handle nested vector constructors (e.g., bvec2(bvec4(...)))
                if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                    let type_name = ident.name.as_str();
                    if type_name.starts_with("bvec") {
                        let nested_dim = match type_name {
                            "bvec2" => 2,
                            "bvec3" => 3,
                            "bvec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_bool_vector_constructor(args, nested_dim)?;
                        components.extend_from_slice(&nested);
                    } else if type_name.starts_with("vec") || type_name.starts_with("ivec") {
                        // Convert numeric vectors to bool (extract components)
                        let nested_dim = match type_name {
                            "vec2" | "ivec2" => 2,
                            "vec3" | "ivec3" => 3,
                            "vec4" | "ivec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_vector_constructor(args, nested_dim)?;
                        // Convert float/int components to bool
                        for val in nested {
                            components.push(val != 0.0);
                        }
                    }
                }
            }
            Expr::Unary(op, unary_expr, _) => {
                use glsl::syntax::UnaryOp;
                if let UnaryOp::Minus = *op {
                    match **unary_expr {
                        Expr::IntConst(n, _) => components.push(-n != 0),
                        Expr::FloatConst(f, _) => components.push(-f != 0.0),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "invalid boolean vector constructor argument",
                            ));
                        }
                    }
                } else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "invalid boolean vector constructor argument",
                    ));
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "invalid boolean vector constructor argument",
                ));
            }
        }
    }

    if components.len() != dim {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "boolean vector constructor expects {} components, got {}",
                dim,
                components.len()
            ),
        ));
    }

    Ok(components)
}

/// Parse a signed integer vector constructor expression into a Vec of i32s
/// Returns a Vec that can be converted to the appropriate array size
fn parse_int_vector_constructor(args: &[Expr], dim: usize) -> Result<Vec<i32>, GlslError> {
    let mut components = Vec::new();

    for arg in args {
        match arg {
            Expr::IntConst(n, _) => components.push(*n),
            Expr::UIntConst(n, _) => {
                // Convert u32 to i32 (clamp to i32::MAX if too large)
                components.push(if *n > i32::MAX as u32 {
                    i32::MAX
                } else {
                    *n as i32
                });
            }
            Expr::FloatConst(f, _) => {
                // Convert float to int: truncate towards zero, clamp to i32 range
                let truncated = trunc_f32(*f);
                let clamped = if truncated < i32::MIN as i64 {
                    i32::MIN
                } else if truncated > i32::MAX as i64 {
                    i32::MAX
                } else {
                    truncated as i32
                };
                components.push(clamped);
            }
            Expr::BoolConst(b, _) => {
                // Convert bool to int: false → 0, true → 1
                components.push(if *b { 1 } else { 0 });
            }
            Expr::FunCall(func_ident, args, _) => {
                // Handle nested vector constructors (e.g., ivec2(ivec4(...)))
                if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                    let type_name = ident.name.as_str();
                    if type_name.starts_with("ivec") {
                        let nested_dim = match type_name {
                            "ivec2" => 2,
                            "ivec3" => 3,
                            "ivec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_int_vector_constructor(args, nested_dim)?;
                        components.extend_from_slice(&nested);
                    } else if type_name.starts_with("uvec") {
                        // Convert unsigned integer vectors to signed (extract components)
                        let nested_dim = match type_name {
                            "uvec2" => 2,
                            "uvec3" => 3,
                            "uvec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_uint_vector_constructor(args, nested_dim)?;
                        // Convert u32 components to i32 (clamp if needed)
                        for val in nested {
                            components.push(if val > i32::MAX as u32 {
                                i32::MAX
                            } else {
                                val as i32
                            });
                        }
                    } else if type_name.starts_with("vec") {
                        // Convert float vectors to int (extract components)
                        let nested_dim = match type_name {
                            "vec2" => 2,
                            "vec3" => 3,
                            "vec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_vector_constructor(args, nested_dim)?;
                        // Convert float components to int
                        for val in nested {
                            let truncated = trunc_f32(val);
                            let clamped = if truncated < i32::MIN as i64 {
                                i32::MIN
                            } else if truncated > i32::MAX as i64 {
                                i32::MAX
                            } else {
                                truncated as i32
                            };
                            components.push(clamped);
                        }
                    } else if type_name.starts_with("bvec") {
                        // Convert boolean vectors to int (extract components)
                        let nested_dim = match type_name {
                            "bvec2" => 2,
                            "bvec3" => 3,
                            "bvec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_bool_vector_constructor(args, nested_dim)?;
                        // Convert bool components to int
                        for val in nested {
                            components.push(if val { 1 } else { 0 });
                        }
                    }
                }
            }
            Expr::Unary(op, unary_expr, _) => {
                use glsl::syntax::UnaryOp;
                if let UnaryOp::Minus = *op {
                    match **unary_expr {
                        Expr::IntConst(n, _) => components.push(-n),
                        Expr::UIntConst(n, _) => {
                            // -5u wraps to large positive in signed, but we'll just negate as i32
                            components.push(-(n as i32));
                        }
                        Expr::FloatConst(f, _) => {
                            // Convert negative float to int
                            let truncated = trunc_f32(-f);
                            let clamped = if truncated < i32::MIN as i64 {
                                i32::MIN
                            } else if truncated > i32::MAX as i64 {
                                i32::MAX
                            } else {
                                truncated as i32
                            };
                            components.push(clamped);
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "invalid integer vector constructor argument",
                            ));
                        }
                    }
                } else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "invalid integer vector constructor argument",
                    ));
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "invalid integer vector constructor argument",
                ));
            }
        }
    }

    if components.len() != dim {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "integer vector constructor expects {} components, got {}",
                dim,
                components.len()
            ),
        ));
    }

    Ok(components)
}

/// Parse a unsigned integer vector constructor expression into a Vec of u32s
/// Returns a Vec that can be converted to the appropriate array size
fn parse_uint_vector_constructor(args: &[Expr], dim: usize) -> Result<Vec<u32>, GlslError> {
    let mut components = Vec::new();

    for arg in args {
        match arg {
            Expr::IntConst(n, _) => {
                // Convert i32 to u32 (bit pattern preserved, but clamp negative values to 0 for safety)
                components.push(if *n < 0 { 0 } else { *n as u32 });
            }
            Expr::UIntConst(n, _) => components.push(*n),
            Expr::FloatConst(f, _) => {
                // Convert float to uint: truncate towards zero, then cast to unsigned
                // Negative values wrap around (e.g., -2.7 -> -2 -> 4294967294u)
                let truncated = trunc_f32(*f) as i32;
                let as_uint = truncated as u32;
                components.push(as_uint);
            }
            Expr::BoolConst(b, _) => {
                // Convert bool to uint: false → 0, true → 1
                components.push(if *b { 1 } else { 0 });
            }
            Expr::FunCall(func_ident, args, _) => {
                // Handle nested vector constructors (e.g., uvec2(uvec4(...)))
                if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                    let type_name = ident.name.as_str();
                    if type_name.starts_with("uvec") {
                        let nested_dim = match type_name {
                            "uvec2" => 2,
                            "uvec3" => 3,
                            "uvec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_uint_vector_constructor(args, nested_dim)?;
                        components.extend_from_slice(&nested);
                    } else if type_name.starts_with("vec") || type_name.starts_with("ivec") {
                        // Convert numeric vectors to uint (extract components)
                        let nested_dim = match type_name {
                            "vec2" | "ivec2" => 2,
                            "vec3" | "ivec3" => 3,
                            "vec4" | "ivec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_vector_constructor(args, nested_dim)?;
                        // Convert float/int components to uint
                        for val in nested {
                            let truncated = trunc_f32(val);
                            let clamped = if truncated < 0 {
                                0
                            } else if truncated > u32::MAX as i64 {
                                u32::MAX
                            } else {
                                truncated as u32
                            };
                            components.push(clamped);
                        }
                    } else if type_name.starts_with("bvec") {
                        // Convert boolean vectors to uint (extract components)
                        let nested_dim = match type_name {
                            "bvec2" => 2,
                            "bvec3" => 3,
                            "bvec4" => 4,
                            _ => continue,
                        };
                        let nested = parse_bool_vector_constructor(args, nested_dim)?;
                        // Convert bool components to uint
                        for val in nested {
                            components.push(if val { 1 } else { 0 });
                        }
                    }
                }
            }
            Expr::Unary(op, unary_expr, _) => {
                use glsl::syntax::UnaryOp;
                if let UnaryOp::Minus = *op {
                    match **unary_expr {
                        Expr::IntConst(n, _) => {
                            // Handle negative integers: -(-5) wraps to large positive in unsigned
                            components.push((-n).wrapping_neg() as u32);
                        }
                        Expr::UIntConst(n, _) => {
                            // -5u wraps to (2^32 - 5) in unsigned arithmetic
                            components.push(n.wrapping_neg());
                        }
                        Expr::FloatConst(f, _) => {
                            // Convert negative float to uint: -5.7 → 4294967291 (wrapping)
                            let truncated = trunc_f32(-f);
                            let wrapped = if truncated >= 0 {
                                (truncated as u32).wrapping_neg()
                            } else {
                                (-truncated as u32).wrapping_neg()
                            };
                            components.push(wrapped);
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "invalid unsigned integer vector constructor argument",
                            ));
                        }
                    }
                } else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "invalid unsigned integer vector constructor argument",
                    ));
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "invalid unsigned integer vector constructor argument",
                ));
            }
        }
    }

    if components.len() != dim {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "unsigned integer vector constructor expects {} components, got {}",
                dim,
                components.len()
            ),
        ));
    }

    Ok(components)
}

/// Parse a matrix constructor expression into a matrix array
/// Supports two formats:
/// 1. Column vectors: mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))
/// 2. Flat scalars: mat2(1.0, 2.0, 3.0, 4.0) - fills in column-major order
/// Returns a fixed-size array that will be converted to the appropriate matrix type
fn parse_matrix_constructor(args: &[Expr], dim: usize) -> Result<[[f32; 4]; 4], GlslError> {
    let mut matrix = [[0.0f32; 4]; 4];

    // Check if all arguments are scalars (flat constructor format)
    let expected_scalar_count = dim * dim;
    let all_scalars = args.iter().all(|arg| {
        matches!(
            arg,
            Expr::FloatConst(_, _)
                | Expr::IntConst(_, _)
                | Expr::Unary(glsl::syntax::UnaryOp::Minus, _, _)
        )
    });

    if all_scalars && args.len() == expected_scalar_count {
        // Flat scalar constructor: mat4(1.0, 2.0, ..., 16.0)
        // Values are filled in column-major order
        let mut components = Vec::new();
        for arg in args {
            match arg {
                Expr::FloatConst(f, _) => components.push(*f),
                Expr::IntConst(n, _) => components.push(*n as f32),
                Expr::Unary(glsl::syntax::UnaryOp::Minus, unary_expr, _) => match **unary_expr {
                    Expr::FloatConst(f, _) => components.push(-f),
                    Expr::IntConst(n, _) => components.push(-(n as f32)),
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "invalid matrix constructor scalar argument",
                        ));
                    }
                },
                _ => {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "invalid matrix constructor scalar argument",
                    ));
                }
            }
        }

        // Fill matrix in column-major order: components[col*dim + row] -> matrix[col][row]
        for col_idx in 0..dim {
            for row_idx in 0..dim {
                matrix[col_idx][row_idx] = components[col_idx * dim + row_idx];
            }
        }

        return Ok(matrix);
    }

    // Column vector constructor: mat2(vec2(...), vec2(...))
    if args.len() != dim {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "matrix constructor expects {} column vectors or {} scalars, got {}",
                dim,
                expected_scalar_count,
                args.len()
            ),
        ));
    }

    for (col_idx, arg) in args.iter().enumerate() {
        match arg {
            Expr::FunCall(func_ident, args, _) => {
                if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                    let type_name = ident.name.as_str();
                    if type_name == "vec2" || type_name == "vec3" || type_name == "vec4" {
                        let vec_dim = match type_name {
                            "vec2" => 2,
                            "vec3" => 3,
                            "vec4" => 4,
                            _ => {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    "invalid matrix column type",
                                ));
                            }
                        };

                        if vec_dim != dim {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "matrix column vector dimension mismatch: expected vec{dim}, got {vec_dim}"
                                ),
                            ));
                        }

                        let vec_components = parse_vector_constructor(args, vec_dim)?;
                        // Store column vector components in the matrix (column-major order)
                        // matrix[col][row] = component from column vector at row position
                        for row_idx in 0..dim {
                            matrix[col_idx][row_idx] = vec_components[row_idx];
                        }
                    } else {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "matrix constructor requires column vectors or scalars",
                        ));
                    }
                } else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "invalid matrix constructor argument",
                    ));
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "matrix constructor requires column vectors or scalars",
                ));
            }
        }
    }

    Ok(matrix)
}

#[cfg(test)]
mod tests {
    use crate::exec::glsl_value::GlslValue;

    #[test]
    fn test_parse_mat2_from_column_vectors() {
        // mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))
        // Column 0: [1.0, 2.0]
        // Column 1: [3.0, 4.0]
        // Storage (column-major): [1.0, 2.0, 3.0, 4.0]
        // Internal representation: [[1.0, 2.0], [3.0, 4.0]] (column-major)
        let result = GlslValue::parse("mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))").unwrap();
        match result {
            GlslValue::Mat2x2(m) => {
                // m[col][row] format
                // Column 0: [1.0, 2.0] (col0_row0, col0_row1)
                // Column 1: [3.0, 4.0] (col1_row0, col1_row1)
                assert_eq!(m[0][0], 1.0); // col0_row0
                assert_eq!(m[0][1], 2.0); // col0_row1
                assert_eq!(m[1][0], 3.0); // col1_row0
                assert_eq!(m[1][1], 4.0); // col1_row1
            }
            _ => panic!("Expected Mat2x2"),
        }
    }

    // Note: Scalar matrix constructors (mat2(1.0, 2.0, 3.0, 4.0)) are not currently
    // supported by GlslValue::parse() which only handles column vector constructors.
    // This is acceptable as column vector constructors are the primary form.
    // Scalar constructors are handled in codegen (constructor.rs) but not in the
    // test value parser.

    #[test]
    fn test_parse_mat3_from_column_vectors() {
        // mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0))
        let result =
            GlslValue::parse("mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0))")
                .unwrap();
        match result {
            GlslValue::Mat3x3(m) => {
                // m[col][row] format
                // Column 0: [1.0, 2.0, 3.0]
                assert_eq!(m[0][0], 1.0); // col0_row0
                assert_eq!(m[0][1], 2.0); // col0_row1
                assert_eq!(m[0][2], 3.0); // col0_row2
                // Column 1: [4.0, 5.0, 6.0]
                assert_eq!(m[1][0], 4.0); // col1_row0
                assert_eq!(m[1][1], 5.0); // col1_row1
                assert_eq!(m[1][2], 6.0); // col1_row2
                // Column 2: [7.0, 8.0, 9.0]
                assert_eq!(m[2][0], 7.0); // col2_row0
                assert_eq!(m[2][1], 8.0); // col2_row1
                assert_eq!(m[2][2], 9.0); // col2_row2
            }
            _ => panic!("Expected Mat3x3"),
        }
    }

    #[test]
    fn test_parse_mat4_from_column_vectors() {
        // mat4 with identity-like pattern
        let result = GlslValue::parse("mat4(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0))").unwrap();
        match result {
            GlslValue::Mat4x4(m) => {
                // m[col][row] format
                // Column 0: [1.0, 0.0, 0.0, 0.0]
                assert_eq!(m[0][0], 1.0); // col0_row0
                assert_eq!(m[0][1], 0.0); // col0_row1
                assert_eq!(m[0][2], 0.0); // col0_row2
                assert_eq!(m[0][3], 0.0); // col0_row3
                // Column 1: [0.0, 1.0, 0.0, 0.0]
                assert_eq!(m[1][0], 0.0); // col1_row0
                assert_eq!(m[1][1], 1.0); // col1_row1
                assert_eq!(m[1][2], 0.0); // col1_row2
                assert_eq!(m[1][3], 0.0); // col1_row3
                // Column 2: [0.0, 0.0, 1.0, 0.0]
                assert_eq!(m[2][0], 0.0); // col2_row0
                assert_eq!(m[2][1], 0.0); // col2_row1
                assert_eq!(m[2][2], 1.0); // col2_row2
                assert_eq!(m[2][3], 0.0); // col2_row3
                // Column 3: [0.0, 0.0, 0.0, 1.0]
                assert_eq!(m[3][0], 0.0); // col3_row0
                assert_eq!(m[3][1], 0.0); // col3_row1
                assert_eq!(m[3][2], 0.0); // col3_row2
                assert_eq!(m[3][3], 1.0); // col3_row3
            }
            _ => panic!("Expected Mat4x4"),
        }
    }

    #[test]
    fn test_flat_array_to_mat2x2_conversion() {
        // Test the conversion logic from test_utils.rs line 71
        // Flat array from emulator (column-major): [col0_row0, col0_row1, col1_row0, col1_row1]
        // For mat2(vec2(1.0, 2.0), vec2(3.0, 4.0)):
        // Storage: [1.0, 2.0, 3.0, 4.0]
        // Conversion: [[v[0], v[1]], [v[2], v[3]]] = [[1.0, 2.0], [3.0, 4.0]]

        let flat_array = vec![1.0, 2.0, 3.0, 4.0];

        // Simulate the conversion from test_utils.rs
        let mat = GlslValue::Mat2x2([
            [flat_array[0], flat_array[1]], // [1.0, 2.0] - col 0
            [flat_array[2], flat_array[3]], // [3.0, 4.0] - col 1
        ]);

        // Verify the matrix represents the correct values
        // Column 0 should be [1.0, 2.0], Column 1 should be [3.0, 4.0]
        match mat {
            GlslValue::Mat2x2(m) => {
                // m[col][row] format
                // Column 0: [m[0][0], m[0][1]] = [1.0, 2.0] ✓
                assert_eq!(m[0][0], 1.0); // col0_row0
                assert_eq!(m[0][1], 2.0); // col0_row1
                // Column 1: [m[1][0], m[1][1]] = [3.0, 4.0] ✓
                assert_eq!(m[1][0], 3.0); // col1_row0
                assert_eq!(m[1][1], 4.0); // col1_row1
            }
            _ => panic!("Expected Mat2x2"),
        }
    }

    #[test]
    fn test_flat_array_to_mat3x3_conversion() {
        // Test the conversion logic from test_utils.rs line 78
        // Flat array (column-major): [col0_row0, col0_row1, col0_row2, col1_row0, col1_row1, col1_row2, col2_row0, col2_row1, col2_row2]
        // For mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0)):
        // Storage: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        // Conversion: [[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]]

        let flat_array = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        // Simulate the conversion from test_utils.rs
        let mat = GlslValue::Mat3x3([
            [flat_array[0], flat_array[1], flat_array[2]], // col 0
            [flat_array[3], flat_array[4], flat_array[5]], // col 1
            [flat_array[6], flat_array[7], flat_array[8]], // col 2
        ]);

        // Verify columns are correct
        match mat {
            GlslValue::Mat3x3(m) => {
                // Column 0: [1.0, 2.0, 3.0]
                assert_eq!(m[0][0], 1.0);
                assert_eq!(m[0][1], 2.0);
                assert_eq!(m[0][2], 3.0);
                // Column 1: [4.0, 5.0, 6.0]
                assert_eq!(m[1][0], 4.0);
                assert_eq!(m[1][1], 5.0);
                assert_eq!(m[1][2], 6.0);
                // Column 2: [7.0, 8.0, 9.0]
                assert_eq!(m[2][0], 7.0);
                assert_eq!(m[2][1], 8.0);
                assert_eq!(m[2][2], 9.0);
            }
            _ => panic!("Expected Mat3x3"),
        }
    }

    #[test]
    fn test_flat_array_to_mat4x4_conversion() {
        // Test the conversion logic from test_utils.rs lines 85-90
        // Flat array (column-major): 16 elements
        // Conversion pattern: [[v[0], v[1], v[2], v[3]], [v[4], v[5], v[6], v[7]], [v[8], v[9], v[10], v[11]], [v[12], v[13], v[14], v[15]]]

        // Identity matrix
        let flat_array = vec![
            1.0, 0.0, 0.0, 0.0, // column 0
            0.0, 1.0, 0.0, 0.0, // column 1
            0.0, 0.0, 1.0, 0.0, // column 2
            0.0, 0.0, 0.0, 1.0, // column 3
        ];

        // Simulate the conversion from test_utils.rs
        let mat = GlslValue::Mat4x4([
            [flat_array[0], flat_array[1], flat_array[2], flat_array[3]], // col 0
            [flat_array[4], flat_array[5], flat_array[6], flat_array[7]], // col 1
            [flat_array[8], flat_array[9], flat_array[10], flat_array[11]], // col 2
            [
                flat_array[12],
                flat_array[13],
                flat_array[14],
                flat_array[15],
            ], // col 3
        ]);

        // Verify columns are correct
        match mat {
            GlslValue::Mat4x4(m) => {
                // Column 0: [1.0, 0.0, 0.0, 0.0]
                assert_eq!(m[0][0], 1.0);
                assert_eq!(m[0][1], 0.0);
                assert_eq!(m[0][2], 0.0);
                assert_eq!(m[0][3], 0.0);
                // Column 1: [0.0, 1.0, 0.0, 0.0]
                assert_eq!(m[1][0], 0.0);
                assert_eq!(m[1][1], 1.0);
                assert_eq!(m[1][2], 0.0);
                assert_eq!(m[1][3], 0.0);
                // Column 2: [0.0, 0.0, 1.0, 0.0]
                assert_eq!(m[2][0], 0.0);
                assert_eq!(m[2][1], 0.0);
                assert_eq!(m[2][2], 1.0);
                assert_eq!(m[2][3], 0.0);
                // Column 3: [0.0, 0.0, 0.0, 1.0]
                assert_eq!(m[3][0], 0.0);
                assert_eq!(m[3][1], 0.0);
                assert_eq!(m[3][2], 0.0);
                assert_eq!(m[3][3], 1.0);
            }
            _ => panic!("Expected Mat4x4"),
        }
    }
}

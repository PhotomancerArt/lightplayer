//! Constant expression evaluation.
//!
//! Evaluates GLSL constant expressions for const initializers and array sizes.
//! Supports literals, binary ops, unary minus, constructors, and const variable references.

use crate::error::{GlslError, source_span_to_location};
use crate::frontend::semantic::types::Type;
use hashbrown::HashMap;

use alloc::format;
use glsl::syntax::Expr;

/// Evaluated constant value for use in const initializers and array sizes.
#[derive(Debug, Clone)]
pub enum ConstValue {
    Int(i32),
    UInt(u32),
    Float(f32),
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
    Mat2([[f32; 2]; 2]),
}

impl ConstValue {
    pub fn glsl_type(&self) -> Type {
        match self {
            ConstValue::Int(_) => Type::Int,
            ConstValue::UInt(_) => Type::UInt,
            ConstValue::Float(_) => Type::Float,
            ConstValue::Bool(_) => Type::Bool,
            ConstValue::Vec2(_) => Type::Vec2,
            ConstValue::Vec3(_) => Type::Vec3,
            ConstValue::Vec4(_) => Type::Vec4,
            ConstValue::IVec2(_) => Type::IVec2,
            ConstValue::IVec3(_) => Type::IVec3,
            ConstValue::IVec4(_) => Type::IVec4,
            ConstValue::UVec2(_) => Type::UVec2,
            ConstValue::UVec3(_) => Type::UVec3,
            ConstValue::UVec4(_) => Type::UVec4,
            ConstValue::BVec2(_) => Type::BVec2,
            ConstValue::BVec3(_) => Type::BVec3,
            ConstValue::BVec4(_) => Type::BVec4,
            ConstValue::Mat2(_) => Type::Mat2,
        }
    }

    /// Convert to i32 for array size. Returns Err if not an integral type.
    pub fn to_array_size(&self) -> Result<usize, GlslError> {
        match self {
            ConstValue::Int(n) => {
                if *n < 0 {
                    return Err(GlslError::new(
                        crate::error::ErrorCode::E0400,
                        format!("array size must be positive, got {n}"),
                    ));
                }
                Ok(*n as usize)
            }
            ConstValue::UInt(n) => Ok(*n as usize),
            _ => Err(GlslError::new(
                crate::error::ErrorCode::E0400,
                format!(
                    "array size must be constant integral expression, got {:?}",
                    self.glsl_type()
                ),
            )),
        }
    }
}

/// Environment mapping const variable names to their evaluated values.
pub type ConstEnv = HashMap<alloc::string::String, ConstValue>;

/// Evaluate a constant expression.
///
/// Supports: literals, variable references (looked up in env), binary ops
/// (+, -, *, /, %), unary minus, and type constructors (vec2, vec3, vec4, mat2, etc).
pub fn eval_constant_expr(
    expr: &Expr,
    const_env: &ConstEnv,
    span: Option<&glsl::syntax::SourceSpan>,
) -> Result<ConstValue, GlslError> {
    let err = |msg: &str| {
        let mut e = GlslError::new(crate::error::ErrorCode::E0400, msg);
        if let Some(s) = span {
            e = e.with_location(source_span_to_location(s));
        }
        e
    };

    match expr {
        Expr::IntConst(n, _) => Ok(ConstValue::Int(*n)),
        Expr::UIntConst(n, _) => Ok(ConstValue::UInt(*n)),
        Expr::FloatConst(f, _) => Ok(ConstValue::Float(*f)),
        Expr::DoubleConst(f, _) => Ok(ConstValue::Float(*f as f32)),
        Expr::BoolConst(b, _) => Ok(ConstValue::Bool(*b)),

        Expr::Variable(ident, _s) => {
            let name = &ident.name;
            const_env
                .get(name)
                .cloned()
                .ok_or_else(|| err(&format!("`{name}` is not a constant expression")))
        }

        Expr::Unary(op, operand, s) => {
            let val = eval_constant_expr(operand, const_env, Some(s))?;
            eval_unary(op, val).map_err(|e| err(e))
        }

        Expr::Binary(op, left, right, s) => {
            let l = eval_constant_expr(left, const_env, Some(s))?;
            let r = eval_constant_expr(right, const_env, Some(s))?;
            eval_binary(op, &l, &r).map_err(|e| err(e))
        }

        Expr::FunCall(func_ident, args, s) => {
            let args_vals: alloc::vec::Vec<ConstValue> = args
                .iter()
                .map(|a| eval_constant_expr(a, const_env, Some(s)))
                .collect::<Result<_, _>>()?;
            match eval_builtin(func_ident, &args_vals) {
                Ok(Some(val)) => Ok(val),
                Ok(None) => eval_constructor(func_ident, &args_vals).map_err(|e| err(e)),
                Err(e) => Err(err(e)),
            }
        }

        _ => Err(err(&format!(
            "expression is not a constant expression: {expr:?}"
        ))),
    }
}

fn eval_unary(op: &glsl::syntax::UnaryOp, val: ConstValue) -> Result<ConstValue, &'static str> {
    use glsl::syntax::UnaryOp;

    match op {
        UnaryOp::Minus => match val {
            ConstValue::Int(n) => Ok(ConstValue::Int(-n)),
            ConstValue::Float(f) => Ok(ConstValue::Float(-f)),
            ConstValue::Vec2(v) => Ok(ConstValue::Vec2([-v[0], -v[1]])),
            ConstValue::Vec3(v) => Ok(ConstValue::Vec3([-v[0], -v[1], -v[2]])),
            ConstValue::Vec4(v) => Ok(ConstValue::Vec4([-v[0], -v[1], -v[2], -v[3]])),
            ConstValue::IVec2(v) => Ok(ConstValue::IVec2([-v[0], -v[1]])),
            ConstValue::IVec3(v) => Ok(ConstValue::IVec3([-v[0], -v[1], -v[2]])),
            ConstValue::IVec4(v) => Ok(ConstValue::IVec4([-v[0], -v[1], -v[2], -v[3]])),
            _ => Err("unary minus requires numeric type"),
        },
        UnaryOp::Not => match val {
            ConstValue::Bool(b) => Ok(ConstValue::Bool(!b)),
            _ => Err("unary not requires bool"),
        },
        UnaryOp::Inc | UnaryOp::Dec => {
            Err("increment/decrement not allowed in constant expression")
        }
        _ => Ok(val), // Unary plus (+x) is identity
    }
}

fn eval_binary(
    op: &glsl::syntax::BinaryOp,
    left: &ConstValue,
    right: &ConstValue,
) -> Result<ConstValue, &'static str> {
    use glsl::syntax::BinaryOp;

    match op {
        BinaryOp::Add => eval_bin_arith(left, right, |a, b| a + b, |a, b| a + b, |a, b| a + b),
        BinaryOp::Sub => eval_bin_arith(left, right, |a, b| a - b, |a, b| a - b, |a, b| a - b),
        BinaryOp::Mult => {
            if let Some(r) = eval_vec_mat_scalar(left, right, |a, b| a * b) {
                return Ok(r);
            }
            eval_bin_arith(left, right, |a, b| a * b, |a, b| a * b, |a, b| a * b)
        }
        BinaryOp::Div => {
            if let Some(r) = eval_vec_mat_scalar_div(left, right) {
                return Ok(r);
            }
            eval_bin_arith(
                left,
                right,
                |a, b| a / b,
                |a, b| a / b,
                |a, b| if b != 0.0 { a / b } else { f32::NAN },
            )
        }
        BinaryOp::Mod => eval_bin_mod(left, right),
        _ => Err("operator not allowed in constant expression"),
    }
}

fn scalar_from_const_for_vec_mat(v: &ConstValue) -> Option<f32> {
    match v {
        ConstValue::Int(n) => Some(*n as f32),
        ConstValue::UInt(n) => Some(*n as f32),
        ConstValue::Float(f) => Some(*f),
        _ => None,
    }
}

/// Vec/mat * scalar or scalar * vec/mat (component-wise).
fn eval_vec_mat_scalar<F>(left: &ConstValue, right: &ConstValue, op: F) -> Option<ConstValue>
where
    F: Fn(f32, f32) -> f32,
{
    let scalar = scalar_from_const_for_vec_mat(right)?;
    match left {
        ConstValue::Vec2(v) => Some(ConstValue::Vec2([op(v[0], scalar), op(v[1], scalar)])),
        ConstValue::Vec3(v) => Some(ConstValue::Vec3([
            op(v[0], scalar),
            op(v[1], scalar),
            op(v[2], scalar),
        ])),
        ConstValue::Vec4(v) => Some(ConstValue::Vec4([
            op(v[0], scalar),
            op(v[1], scalar),
            op(v[2], scalar),
            op(v[3], scalar),
        ])),
        ConstValue::Mat2(m) => Some(ConstValue::Mat2([
            [op(m[0][0], scalar), op(m[0][1], scalar)],
            [op(m[1][0], scalar), op(m[1][1], scalar)],
        ])),
        _ => None,
    }
    .or_else(|| {
        let scalar = scalar_from_const_for_vec_mat(left)?;
        match right {
            ConstValue::Vec2(v) => Some(ConstValue::Vec2([op(scalar, v[0]), op(scalar, v[1])])),
            ConstValue::Vec3(v) => Some(ConstValue::Vec3([
                op(scalar, v[0]),
                op(scalar, v[1]),
                op(scalar, v[2]),
            ])),
            ConstValue::Vec4(v) => Some(ConstValue::Vec4([
                op(scalar, v[0]),
                op(scalar, v[1]),
                op(scalar, v[2]),
                op(scalar, v[3]),
            ])),
            ConstValue::Mat2(m) => Some(ConstValue::Mat2([
                [op(scalar, m[0][0]), op(scalar, m[0][1])],
                [op(scalar, m[1][0]), op(scalar, m[1][1])],
            ])),
            _ => None,
        }
    })
}

/// Vec/mat / scalar (scalar/vec not defined in GLSL).
fn eval_vec_mat_scalar_div(left: &ConstValue, right: &ConstValue) -> Option<ConstValue> {
    let scalar = scalar_from_const_for_vec_mat(right)?;
    if scalar == 0.0 {
        return None;
    }
    match left {
        ConstValue::Vec2(v) => Some(ConstValue::Vec2([v[0] / scalar, v[1] / scalar])),
        ConstValue::Vec3(v) => Some(ConstValue::Vec3([
            v[0] / scalar,
            v[1] / scalar,
            v[2] / scalar,
        ])),
        ConstValue::Vec4(v) => Some(ConstValue::Vec4([
            v[0] / scalar,
            v[1] / scalar,
            v[2] / scalar,
            v[3] / scalar,
        ])),
        ConstValue::Mat2(m) => Some(ConstValue::Mat2([
            [m[0][0] / scalar, m[0][1] / scalar],
            [m[1][0] / scalar, m[1][1] / scalar],
        ])),
        _ => None,
    }
}

fn eval_bin_arith<FI, FF>(
    left: &ConstValue,
    right: &ConstValue,
    int_op: FI,
    uint_op: fn(u32, u32) -> u32,
    float_op: FF,
) -> Result<ConstValue, &'static str>
where
    FI: Fn(i32, i32) -> i32,
    FF: Fn(f32, f32) -> f32,
{
    match (left, right) {
        (ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Int(int_op(*a, *b))),
        (ConstValue::UInt(a), ConstValue::UInt(b)) => Ok(ConstValue::UInt(uint_op(*a, *b))),
        (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(float_op(*a, *b))),
        (ConstValue::Vec2(a), ConstValue::Vec2(b)) => Ok(ConstValue::Vec2([
            float_op(a[0], b[0]),
            float_op(a[1], b[1]),
        ])),
        (ConstValue::Vec3(a), ConstValue::Vec3(b)) => Ok(ConstValue::Vec3([
            float_op(a[0], b[0]),
            float_op(a[1], b[1]),
            float_op(a[2], b[2]),
        ])),
        (ConstValue::Vec4(a), ConstValue::Vec4(b)) => Ok(ConstValue::Vec4([
            float_op(a[0], b[0]),
            float_op(a[1], b[1]),
            float_op(a[2], b[2]),
            float_op(a[3], b[3]),
        ])),
        (ConstValue::IVec2(a), ConstValue::IVec2(b)) => {
            Ok(ConstValue::IVec2([int_op(a[0], b[0]), int_op(a[1], b[1])]))
        }
        (ConstValue::IVec3(a), ConstValue::IVec3(b)) => Ok(ConstValue::IVec3([
            int_op(a[0], b[0]),
            int_op(a[1], b[1]),
            int_op(a[2], b[2]),
        ])),
        (ConstValue::IVec4(a), ConstValue::IVec4(b)) => Ok(ConstValue::IVec4([
            int_op(a[0], b[0]),
            int_op(a[1], b[1]),
            int_op(a[2], b[2]),
            int_op(a[3], b[3]),
        ])),
        _ => Err("incompatible types for binary operator"),
    }
}

fn eval_bin_mod(left: &ConstValue, right: &ConstValue) -> Result<ConstValue, &'static str> {
    match (left, right) {
        (ConstValue::Int(a), ConstValue::Int(b)) => {
            if *b == 0 {
                return Err("modulo by zero");
            }
            Ok(ConstValue::Int(a % b))
        }
        (ConstValue::UInt(a), ConstValue::UInt(b)) => {
            if *b == 0 {
                return Err("modulo by zero");
            }
            Ok(ConstValue::UInt(a % b))
        }
        (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a % b)),
        _ => Err("modulo requires integral or float types"),
    }
}

/// Evaluate builtin function call.
/// Returns Ok(Some(val)) if builtin evaluated, Ok(None) if not a builtin, Err if builtin with error.
fn eval_builtin(
    func_ident: &glsl::syntax::FunIdentifier,
    args: &[ConstValue],
) -> Result<Option<ConstValue>, &'static str> {
    use glsl::syntax::FunIdentifier;

    let name = match func_ident {
        FunIdentifier::Identifier(ident) => ident.name.as_str(),
        _ => return Ok(None),
    };

    let result = match name {
        "abs" => {
            if args.len() != 1 {
                return Err("abs requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "abs requires numeric")?;
            Ok(Some(ConstValue::Float(x.abs())))
        }
        "sign" => {
            if args.len() != 1 {
                return Err("sign requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "sign requires numeric")?;
            Ok(Some(ConstValue::Float(if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            })))
        }
        "floor" => {
            if args.len() != 1 {
                return Err("floor requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "floor requires numeric")?;
            Ok(Some(ConstValue::Float(libm::floorf(x))))
        }
        "trunc" => {
            if args.len() != 1 {
                return Err("trunc requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "trunc requires numeric")?;
            Ok(Some(ConstValue::Float(libm::truncf(x))))
        }
        "round" => {
            if args.len() != 1 {
                return Err("round requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "round requires numeric")?;
            Ok(Some(ConstValue::Float(libm::roundf(x))))
        }
        "ceil" => {
            if args.len() != 1 {
                return Err("ceil requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "ceil requires numeric")?;
            Ok(Some(ConstValue::Float(libm::ceilf(x))))
        }
        "min" => {
            if args.len() != 2 {
                return Err("min requires 2 arguments");
            }
            let a = scalar_from_const(&args[0]).map_err(|_| "min requires numeric")?;
            let b = scalar_from_const(&args[1]).map_err(|_| "min requires numeric")?;
            Ok(Some(ConstValue::Float(a.min(b))))
        }
        "max" => {
            if args.len() != 2 {
                return Err("max requires 2 arguments");
            }
            let a = scalar_from_const(&args[0]).map_err(|_| "max requires numeric")?;
            let b = scalar_from_const(&args[1]).map_err(|_| "max requires numeric")?;
            Ok(Some(ConstValue::Float(a.max(b))))
        }
        "clamp" => {
            if args.len() != 3 {
                return Err("clamp requires 3 arguments");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "clamp requires numeric")?;
            let lo = scalar_from_const(&args[1]).map_err(|_| "clamp requires numeric")?;
            let hi = scalar_from_const(&args[2]).map_err(|_| "clamp requires numeric")?;
            Ok(Some(ConstValue::Float(x.clamp(lo, hi))))
        }
        "mod" => {
            if args.len() != 2 {
                return Err("mod requires 2 arguments");
            }
            let a = scalar_from_const(&args[0]).map_err(|_| "mod requires numeric")?;
            let b = scalar_from_const(&args[1]).map_err(|_| "mod requires numeric")?;
            Ok(Some(ConstValue::Float(a - libm::floorf(a / b) * b)))
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err("sqrt requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "sqrt requires numeric")?;
            Ok(Some(ConstValue::Float(libm::sqrtf(x))))
        }
        "inversesqrt" => {
            if args.len() != 1 {
                return Err("inversesqrt requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "inversesqrt requires numeric")?;
            Ok(Some(ConstValue::Float(1.0 / libm::sqrtf(x))))
        }
        "pow" => {
            if args.len() != 2 {
                return Err("pow requires 2 arguments");
            }
            let base = scalar_from_const(&args[0]).map_err(|_| "pow requires numeric")?;
            let exp = scalar_from_const(&args[1]).map_err(|_| "pow requires numeric")?;
            Ok(Some(ConstValue::Float(libm::powf(base, exp))))
        }
        "exp" => {
            if args.len() != 1 {
                return Err("exp requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "exp requires numeric")?;
            Ok(Some(ConstValue::Float(libm::expf(x))))
        }
        "log" => {
            if args.len() != 1 {
                return Err("log requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "log requires numeric")?;
            Ok(Some(ConstValue::Float(libm::logf(x))))
        }
        "exp2" => {
            if args.len() != 1 {
                return Err("exp2 requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "exp2 requires numeric")?;
            Ok(Some(ConstValue::Float(libm::exp2f(x))))
        }
        "log2" => {
            if args.len() != 1 {
                return Err("log2 requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "log2 requires numeric")?;
            Ok(Some(ConstValue::Float(libm::log2f(x))))
        }
        "radians" => {
            if args.len() != 1 {
                return Err("radians requires 1 argument");
            }
            let d = scalar_from_const(&args[0]).map_err(|_| "radians requires numeric")?;
            Ok(Some(ConstValue::Float(d * core::f32::consts::PI / 180.0)))
        }
        "degrees" => {
            if args.len() != 1 {
                return Err("degrees requires 1 argument");
            }
            let r = scalar_from_const(&args[0]).map_err(|_| "degrees requires numeric")?;
            Ok(Some(ConstValue::Float(r * 180.0 / core::f32::consts::PI)))
        }
        "sin" => {
            if args.len() != 1 {
                return Err("sin requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "sin requires numeric")?;
            Ok(Some(ConstValue::Float(libm::sinf(x))))
        }
        "cos" => {
            if args.len() != 1 {
                return Err("cos requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "cos requires numeric")?;
            Ok(Some(ConstValue::Float(libm::cosf(x))))
        }
        "asin" => {
            if args.len() != 1 {
                return Err("asin requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "asin requires numeric")?;
            Ok(Some(ConstValue::Float(libm::asinf(x))))
        }
        "acos" => {
            if args.len() != 1 {
                return Err("acos requires 1 argument");
            }
            let x = scalar_from_const(&args[0]).map_err(|_| "acos requires numeric")?;
            Ok(Some(ConstValue::Float(libm::acosf(x))))
        }
        "length" => {
            if args.len() != 1 {
                return Err("length requires 1 argument");
            }
            let len = match &args[0] {
                ConstValue::Float(f) => *f,
                ConstValue::Vec2(v) => libm::sqrtf(v[0] * v[0] + v[1] * v[1]),
                ConstValue::Vec3(v) => libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]),
                ConstValue::Vec4(v) => {
                    libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[3] * v[3])
                }
                _ => return Err("length requires float or vector"),
            };
            Ok(Some(ConstValue::Float(len)))
        }
        "dot" => {
            if args.len() != 2 {
                return Err("dot requires 2 arguments");
            }
            let d = match (&args[0], &args[1]) {
                (ConstValue::Float(a), ConstValue::Float(b)) => a * b,
                (ConstValue::Vec2(a), ConstValue::Vec2(b)) => a[0] * b[0] + a[1] * b[1],
                (ConstValue::Vec3(a), ConstValue::Vec3(b)) => {
                    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
                }
                (ConstValue::Vec4(a), ConstValue::Vec4(b)) => {
                    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
                }
                _ => return Err("dot requires matching float or vector types"),
            };
            Ok(Some(ConstValue::Float(d)))
        }
        "normalize" => {
            if args.len() != 1 {
                return Err("normalize requires 1 argument");
            }
            let len = match &args[0] {
                ConstValue::Float(f) => *f,
                ConstValue::Vec2(v) => libm::sqrtf(v[0] * v[0] + v[1] * v[1]),
                ConstValue::Vec3(v) => libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]),
                ConstValue::Vec4(v) => {
                    libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[3] * v[3])
                }
                _ => return Err("normalize requires float or vector"),
            };
            if len == 0.0 {
                return Err("normalize of zero vector");
            }
            let inv = 1.0 / len;
            let result = match &args[0] {
                ConstValue::Float(_) => ConstValue::Float(1.0),
                ConstValue::Vec2(v) => ConstValue::Vec2([v[0] * inv, v[1] * inv]),
                ConstValue::Vec3(v) => ConstValue::Vec3([v[0] * inv, v[1] * inv, v[2] * inv]),
                ConstValue::Vec4(v) => {
                    ConstValue::Vec4([v[0] * inv, v[1] * inv, v[2] * inv, v[3] * inv])
                }
                _ => return Err("normalize requires float or vector"),
            };
            Ok(Some(result))
        }
        _ => return Ok(None),
    };

    result
}

fn eval_constructor(
    func_ident: &glsl::syntax::FunIdentifier,
    args: &[ConstValue],
) -> Result<ConstValue, &'static str> {
    use glsl::syntax::FunIdentifier;

    let name = match func_ident {
        FunIdentifier::Identifier(ident) => &ident.name,
        _ => return Err("constructor must be identifier"),
    };

    match name.as_str() {
        "vec2" => {
            if args.len() != 2 {
                return Err("vec2 requires 2 arguments");
            }
            let a = scalar_from_const(&args[0])?;
            let b = scalar_from_const(&args[1])?;
            Ok(ConstValue::Vec2([a, b]))
        }
        "vec3" => {
            if args.len() != 3 {
                return Err("vec3 requires 3 arguments");
            }
            let a = scalar_from_const(&args[0])?;
            let b = scalar_from_const(&args[1])?;
            let c = scalar_from_const(&args[2])?;
            Ok(ConstValue::Vec3([a, b, c]))
        }
        "vec4" => {
            if args.len() != 4 {
                return Err("vec4 requires 4 arguments");
            }
            let a = scalar_from_const(&args[0])?;
            let b = scalar_from_const(&args[1])?;
            let c = scalar_from_const(&args[2])?;
            let d = scalar_from_const(&args[3])?;
            Ok(ConstValue::Vec4([a, b, c, d]))
        }
        "ivec2" => {
            if args.len() != 2 {
                return Err("ivec2 requires 2 arguments");
            }
            let a = int_from_const(&args[0])?;
            let b = int_from_const(&args[1])?;
            Ok(ConstValue::IVec2([a, b]))
        }
        "ivec3" => {
            if args.len() != 3 {
                return Err("ivec3 requires 3 arguments");
            }
            let a = int_from_const(&args[0])?;
            let b = int_from_const(&args[1])?;
            let c = int_from_const(&args[2])?;
            Ok(ConstValue::IVec3([a, b, c]))
        }
        "ivec4" => {
            if args.len() != 4 {
                return Err("ivec4 requires 4 arguments");
            }
            let a = int_from_const(&args[0])?;
            let b = int_from_const(&args[1])?;
            let c = int_from_const(&args[2])?;
            let d = int_from_const(&args[3])?;
            Ok(ConstValue::IVec4([a, b, c, d]))
        }
        "uvec2" | "uvec3" | "uvec4" => {
            return Err("uvec/bvec constructors in const not yet implemented");
        }
        "bvec2" | "bvec3" | "bvec4" => {
            return Err("bvec constructors in const not yet implemented");
        }
        "mat2" => {
            if args.len() != 4 {
                return Err("mat2 requires 4 arguments");
            }
            let a = scalar_from_const(&args[0])?;
            let b = scalar_from_const(&args[1])?;
            let c = scalar_from_const(&args[2])?;
            let d = scalar_from_const(&args[3])?;
            Ok(ConstValue::Mat2([[a, b], [c, d]]))
        }
        "mat3" | "mat4" => return Err("mat3/mat4 in const not yet implemented"),
        "int" => {
            if args.len() != 1 {
                return Err("int constructor requires 1 argument");
            }
            Ok(ConstValue::Int(int_from_const(&args[0])?))
        }
        "uint" => {
            if args.len() != 1 {
                return Err("uint constructor requires 1 argument");
            }
            Ok(ConstValue::UInt(uint_from_const(&args[0])?))
        }
        "float" => {
            if args.len() != 1 {
                return Err("float constructor requires 1 argument");
            }
            Ok(ConstValue::Float(scalar_from_const(&args[0])?))
        }
        "bool" => {
            if args.len() != 1 {
                return Err("bool constructor requires 1 argument");
            }
            Ok(ConstValue::Bool(bool_from_const(&args[0])?))
        }
        _ => Err("unknown constructor or non-const function"),
    }
}

fn scalar_from_const(v: &ConstValue) -> Result<f32, &'static str> {
    match v {
        ConstValue::Int(n) => Ok(*n as f32),
        ConstValue::UInt(n) => Ok(*n as f32),
        ConstValue::Float(f) => Ok(*f),
        ConstValue::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        ConstValue::Vec2(v) => Ok(v[0]),
        ConstValue::Vec3(v) => Ok(v[0]),
        ConstValue::Vec4(v) => Ok(v[0]),
        _ => Err("cannot convert to float"),
    }
}

fn int_from_const(v: &ConstValue) -> Result<i32, &'static str> {
    match v {
        ConstValue::Int(n) => Ok(*n),
        ConstValue::Float(f) => Ok(*f as i32),
        ConstValue::Bool(b) => Ok(if *b { 1 } else { 0 }),
        ConstValue::Vec2(v) => Ok(v[0] as i32),
        ConstValue::Vec3(v) => Ok(v[0] as i32),
        ConstValue::Vec4(v) => Ok(v[0] as i32),
        _ => Err("cannot convert to int"),
    }
}

fn uint_from_const(v: &ConstValue) -> Result<u32, &'static str> {
    match v {
        ConstValue::UInt(n) => Ok(*n),
        ConstValue::Int(n) => Ok(*n as u32),
        ConstValue::Float(f) => Ok(*f as u32),
        _ => Err("cannot convert to uint"),
    }
}

fn bool_from_const(v: &ConstValue) -> Result<bool, &'static str> {
    match v {
        ConstValue::Bool(b) => Ok(*b),
        ConstValue::Int(n) => Ok(*n != 0),
        ConstValue::Float(f) => Ok(*f != 0.0),
        _ => Err("cannot convert to bool"),
    }
}

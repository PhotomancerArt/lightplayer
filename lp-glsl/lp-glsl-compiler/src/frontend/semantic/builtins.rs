//! GLSL built-in function signatures and type checking

use crate::frontend::semantic::types::Type;

use alloc::string::{String, ToString};
use alloc::{format, vec, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinSignature {
    pub name: &'static str,
    pub param_types: Vec<BuiltinParamType>,
    pub return_type: BuiltinReturnType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinParamType {
    GenFType,   // float, vec2, vec3, vec4
    GenIType,   // int, ivec2, ivec3, ivec4
    GenUType,   // uint, uvec2, uvec3, uvec4
    GenBType,   // bool, bvec2, bvec3, bvec4
    GenMatType, // mat2, mat3, mat4
    Float,      // scalar float only
    Int,        // scalar int only
    UInt,       // scalar uint only
    Vec3,       // vec3 only (for cross product)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinReturnType {
    SameAsParam(usize), // Return type matches parameter N
    AlwaysFloat,        // Always returns float (length, dot)
    AlwaysVec3,         // Always returns vec3 (cross)
    AlwaysBool,         // Always returns bool (all, any)
}

/// Check if a name is a built-in function
pub fn is_builtin_function(name: &str) -> bool {
    lookup_builtin(name).is_some()
}

/// Lookup built-in function signatures by name
pub fn lookup_builtin(name: &str) -> Option<Vec<BuiltinSignature>> {
    match name {
        "dot" => Some(vec![BuiltinSignature {
            name: "dot",
            param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::AlwaysFloat,
        }]),

        "cross" => Some(vec![BuiltinSignature {
            name: "cross",
            param_types: vec![BuiltinParamType::Vec3, BuiltinParamType::Vec3],
            return_type: BuiltinReturnType::AlwaysVec3,
        }]),

        "length" => Some(vec![BuiltinSignature {
            name: "length",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::AlwaysFloat,
        }]),

        "normalize" => Some(vec![BuiltinSignature {
            name: "normalize",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "distance" => Some(vec![BuiltinSignature {
            name: "distance",
            param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::AlwaysFloat,
        }]),

        "min" => Some(vec![
            BuiltinSignature {
                name: "min",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "min",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::Float],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "min",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "min",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::Int],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "min",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "min",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::UInt],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "max" => Some(vec![
            BuiltinSignature {
                name: "max",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "max",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::Float],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "max",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "max",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::Int],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "max",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "max",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::UInt],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "clamp" => Some(vec![
            BuiltinSignature {
                name: "clamp",
                param_types: vec![
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "clamp",
                param_types: vec![
                    BuiltinParamType::GenFType,
                    BuiltinParamType::Float,
                    BuiltinParamType::Float,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "clamp",
                param_types: vec![
                    BuiltinParamType::GenIType,
                    BuiltinParamType::GenIType,
                    BuiltinParamType::GenIType,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "clamp",
                param_types: vec![
                    BuiltinParamType::GenIType,
                    BuiltinParamType::Int,
                    BuiltinParamType::Int,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "abs" => Some(vec![
            BuiltinSignature {
                name: "abs",
                param_types: vec![BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "abs",
                param_types: vec![BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "sqrt" => Some(vec![BuiltinSignature {
            name: "sqrt",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "inversesqrt" => Some(vec![BuiltinSignature {
            name: "inversesqrt",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "exp" => Some(vec![BuiltinSignature {
            name: "exp",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "log" => Some(vec![BuiltinSignature {
            name: "log",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "exp2" => Some(vec![BuiltinSignature {
            name: "exp2",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "log2" => Some(vec![BuiltinSignature {
            name: "log2",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        // Angle and Trigonometry Functions (builtinfunctions.adoc:122-310)
        "radians" => Some(vec![BuiltinSignature {
            name: "radians",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "degrees" => Some(vec![BuiltinSignature {
            name: "degrees",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "sin" => Some(vec![BuiltinSignature {
            name: "sin",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "cos" => Some(vec![BuiltinSignature {
            name: "cos",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "tan" => Some(vec![BuiltinSignature {
            name: "tan",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "asin" => Some(vec![BuiltinSignature {
            name: "asin",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "acos" => Some(vec![BuiltinSignature {
            name: "acos",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "atan" => Some(vec![
            BuiltinSignature {
                name: "atan",
                param_types: vec![BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "atan",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "sinh" => Some(vec![BuiltinSignature {
            name: "sinh",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "cosh" => Some(vec![BuiltinSignature {
            name: "cosh",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "tanh" => Some(vec![BuiltinSignature {
            name: "tanh",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "asinh" => Some(vec![BuiltinSignature {
            name: "asinh",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "acosh" => Some(vec![BuiltinSignature {
            name: "acosh",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "atanh" => Some(vec![BuiltinSignature {
            name: "atanh",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "floor" => Some(vec![BuiltinSignature {
            name: "floor",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "ceil" => Some(vec![BuiltinSignature {
            name: "ceil",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "round" => Some(vec![BuiltinSignature {
            name: "round",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "roundEven" => Some(vec![BuiltinSignature {
            name: "roundEven",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "pow" => Some(vec![BuiltinSignature {
            name: "pow",
            param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "mix" => Some(vec![
            BuiltinSignature {
                name: "mix",
                param_types: vec![
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "mix",
                param_types: vec![
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                    BuiltinParamType::Float,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "mix",
                param_types: vec![
                    BuiltinParamType::GenBType,
                    BuiltinParamType::GenBType,
                    BuiltinParamType::GenBType,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "step" => Some(vec![
            BuiltinSignature {
                name: "step",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "step",
                param_types: vec![BuiltinParamType::Float, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(1),
            },
        ]),

        "smoothstep" => Some(vec![
            BuiltinSignature {
                name: "smoothstep",
                param_types: vec![
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                    BuiltinParamType::GenFType,
                ],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "smoothstep",
                param_types: vec![
                    BuiltinParamType::Float,
                    BuiltinParamType::Float,
                    BuiltinParamType::GenFType,
                ],
                return_type: BuiltinReturnType::SameAsParam(2),
            },
        ]),

        "fract" => Some(vec![BuiltinSignature {
            name: "fract",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "mod" => Some(vec![
            BuiltinSignature {
                name: "mod",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "mod",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::Float],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "sign" => Some(vec![
            BuiltinSignature {
                name: "sign",
                param_types: vec![BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "sign",
                param_types: vec![BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "isinf" => Some(vec![BuiltinSignature {
            name: "isinf",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::AlwaysBool,
        }]),

        "isnan" => Some(vec![BuiltinSignature {
            name: "isnan",
            param_types: vec![BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::AlwaysBool,
        }]),

        // Matrix Functions (builtinfunctions.adoc:1538-1687)
        "matrixCompMult" => Some(vec![BuiltinSignature {
            name: "matrixCompMult",
            param_types: vec![BuiltinParamType::GenMatType, BuiltinParamType::GenMatType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "outerProduct" => Some(vec![BuiltinSignature {
            name: "outerProduct",
            param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
            return_type: BuiltinReturnType::SameAsParam(0), // Returns matrix based on vector sizes
        }]),

        "transpose" => Some(vec![BuiltinSignature {
            name: "transpose",
            param_types: vec![BuiltinParamType::GenMatType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "determinant" => Some(vec![BuiltinSignature {
            name: "determinant",
            param_types: vec![BuiltinParamType::GenMatType],
            return_type: BuiltinReturnType::AlwaysFloat,
        }]),

        "inverse" => Some(vec![BuiltinSignature {
            name: "inverse",
            param_types: vec![BuiltinParamType::GenMatType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        // Relational Functions
        "all" => Some(vec![BuiltinSignature {
            name: "all",
            param_types: vec![BuiltinParamType::GenBType],
            return_type: BuiltinReturnType::AlwaysBool,
        }]),

        "any" => Some(vec![BuiltinSignature {
            name: "any",
            param_types: vec![BuiltinParamType::GenBType],
            return_type: BuiltinReturnType::AlwaysBool,
        }]),

        "not" => Some(vec![BuiltinSignature {
            name: "not",
            param_types: vec![BuiltinParamType::GenBType],
            return_type: BuiltinReturnType::SameAsParam(0),
        }]),

        "equal" => Some(vec![
            BuiltinSignature {
                name: "equal",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "equal",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "equal",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "equal",
                param_types: vec![BuiltinParamType::GenBType, BuiltinParamType::GenBType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "notEqual" => Some(vec![
            BuiltinSignature {
                name: "notEqual",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "notEqual",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "notEqual",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "notEqual",
                param_types: vec![BuiltinParamType::GenBType, BuiltinParamType::GenBType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "greaterThan" => Some(vec![
            BuiltinSignature {
                name: "greaterThan",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "greaterThan",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "greaterThan",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "greaterThanEqual" => Some(vec![
            BuiltinSignature {
                name: "greaterThanEqual",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "greaterThanEqual",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "greaterThanEqual",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "lessThan" => Some(vec![
            BuiltinSignature {
                name: "lessThan",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "lessThan",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "lessThan",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        "lessThanEqual" => Some(vec![
            BuiltinSignature {
                name: "lessThanEqual",
                param_types: vec![BuiltinParamType::GenFType, BuiltinParamType::GenFType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "lessThanEqual",
                param_types: vec![BuiltinParamType::GenIType, BuiltinParamType::GenIType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
            BuiltinSignature {
                name: "lessThanEqual",
                param_types: vec![BuiltinParamType::GenUType, BuiltinParamType::GenUType],
                return_type: BuiltinReturnType::SameAsParam(0),
            },
        ]),

        _ => None,
    }
}

/// Type check a built-in function call and return the result type
pub fn check_builtin_call(name: &str, arg_types: &[Type]) -> Result<Type, String> {
    let signatures =
        lookup_builtin(name).ok_or_else(|| format!("Unknown built-in function: {name}"))?;

    // Try each overload
    for sig in &signatures {
        if let Ok(return_type) = try_match_signature(sig, arg_types) {
            return Ok(return_type);
        }
    }

    Err(format!("No matching overload for {name}({arg_types:?})"))
}

/// Try to match a function signature with provided argument types
fn try_match_signature(sig: &BuiltinSignature, arg_types: &[Type]) -> Result<Type, String> {
    // Check argument count
    if arg_types.len() != sig.param_types.len() {
        return Err("Argument count mismatch".to_string());
    }

    // Check each parameter type
    for (i, (param_ty, arg_ty)) in sig.param_types.iter().zip(arg_types).enumerate() {
        if !matches_param_type(param_ty, arg_ty) {
            return Err(format!("Parameter {i} type mismatch"));
        }
    }

    // Validate genType consistency (all genFType params must have same size)
    validate_gentype_consistency(&sig.param_types, arg_types)?;

    // Compute return type
    let return_type = match sig.return_type {
        BuiltinReturnType::SameAsParam(idx) => {
            // Special case: outerProduct returns matrix based on vector sizes
            if sig.name == "outerProduct" {
                let vec1_size = arg_types[0].component_count().unwrap_or(1);
                let vec2_size = arg_types[1].component_count().unwrap_or(1);
                // outerProduct(vec1, vec2) returns mat(vec1_size × vec2_size)
                // For now, we only support square matrices, so require matching sizes
                if vec1_size == vec2_size {
                    match vec1_size {
                        2 => Type::Mat2,
                        3 => Type::Mat3,
                        4 => Type::Mat4,
                        _ => {
                            return Err(format!(
                                "outerProduct: unsupported vector size {vec1_size}"
                            ));
                        }
                    }
                } else {
                    return Err(format!(
                        "outerProduct: vector sizes must match for square matrices (got {vec1_size} and {vec2_size})"
                    ));
                }
            } else {
                arg_types[idx].clone()
            }
        }
        BuiltinReturnType::AlwaysFloat => Type::Float,
        BuiltinReturnType::AlwaysVec3 => Type::Vec3,
        BuiltinReturnType::AlwaysBool => {
            // Special case: isinf/isnan return bool vectors for vector inputs
            if sig.name == "isinf" || sig.name == "isnan" {
                let input_ty = &arg_types[0];
                if input_ty.is_vector() {
                    let dim = input_ty.component_count().unwrap();
                    match dim {
                        2 => Type::BVec2,
                        3 => Type::BVec3,
                        4 => Type::BVec4,
                        _ => Type::Bool, // Fallback (shouldn't happen)
                    }
                } else {
                    Type::Bool
                }
            } else {
                Type::Bool
            }
        }
    };

    Ok(return_type)
}

/// Check if an argument type matches a parameter type specification
fn matches_param_type(param: &BuiltinParamType, arg: &Type) -> bool {
    match param {
        BuiltinParamType::GenFType => {
            matches!(arg, Type::Float | Type::Vec2 | Type::Vec3 | Type::Vec4)
        }
        BuiltinParamType::GenIType => {
            matches!(arg, Type::Int | Type::IVec2 | Type::IVec3 | Type::IVec4)
        }
        BuiltinParamType::GenUType => {
            matches!(arg, Type::UInt | Type::UVec2 | Type::UVec3 | Type::UVec4)
        }
        BuiltinParamType::GenBType => {
            matches!(arg, Type::Bool | Type::BVec2 | Type::BVec3 | Type::BVec4)
        }
        BuiltinParamType::GenMatType => matches!(arg, Type::Mat2 | Type::Mat3 | Type::Mat4),
        BuiltinParamType::Float => arg == &Type::Float,
        BuiltinParamType::Int => arg == &Type::Int,
        BuiltinParamType::UInt => arg == &Type::UInt,
        BuiltinParamType::Vec3 => arg == &Type::Vec3,
    }
}

/// Validate that all GenFType/GenIType parameters have consistent sizes
fn validate_gentype_consistency(
    param_types: &[BuiltinParamType],
    arg_types: &[Type],
) -> Result<(), String> {
    // Find the first genType parameter to establish the expected type
    let mut expected_type: Option<Type> = None;

    for (param, arg) in param_types.iter().zip(arg_types) {
        match param {
            BuiltinParamType::GenFType
            | BuiltinParamType::GenIType
            | BuiltinParamType::GenBType
            | BuiltinParamType::GenMatType => {
                if let Some(ref expected) = expected_type {
                    if arg != expected {
                        return Err(format!(
                            "GenType parameter type mismatch: expected {expected:?}, got {arg:?}"
                        ));
                    }
                } else {
                    expected_type = Some(arg.clone());
                }
            }
            _ => {}
        }
    }

    Ok(())
}

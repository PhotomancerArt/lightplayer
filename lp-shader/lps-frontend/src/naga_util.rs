//! Naga IR: LPIR type layout, widths, and expression [`TypeInner`] / [`ScalarKind`] helpers.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::IrType;
use lps_shared::LayoutRules;
use naga::{
    ArraySize, BinaryOperator, Expression, Function, Handle, Literal, MathFunction, Module,
    RelationalFunction, Scalar, ScalarKind, Type, TypeInner, VectorSize,
};
use smallvec::SmallVec;

use crate::lower_error::LowerError;

pub(crate) type IrTypeVec = SmallVec<[IrType; 4]>;

/// One struct member: std430 [`byte_offset`], Naga and LPS types, and flat IR types for
/// direct scalar/vector/matrix members (empty when the member is a nested aggregate).
#[derive(Clone, Debug)]
pub(crate) struct MemberInfo {
    pub byte_offset: u32,
    pub naga_ty: Handle<Type>,
    pub lps_ty: lps_shared::LpsType,
    pub ir_tys: IrTypeVec,
}

fn align_u32(offset: u32, align: u32) -> u32 {
    debug_assert!(align > 0);
    let a = align as u64;
    let o = offset as u64;
    (((o + a - 1) / a) * a) as u32
}

pub(crate) fn vector_size_usize(size: VectorSize) -> usize {
    match size {
        VectorSize::Bi => 2,
        VectorSize::Tri => 3,
        VectorSize::Quad => 4,
    }
}

pub(crate) fn naga_scalar_to_ir_type(kind: ScalarKind) -> Result<IrType, LowerError> {
    match kind {
        ScalarKind::Float => Ok(IrType::F32),
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => Ok(IrType::I32),
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(LowerError::UnsupportedType(
            String::from("abstract numeric type"),
        )),
    }
}

/// Flatten a Naga type to LPIR slot component types (std430 order). `module` is required for
/// [`TypeInner::Struct`].
pub(crate) fn naga_type_to_ir_types(
    module: &Module,
    inner: &TypeInner,
) -> Result<IrTypeVec, LowerError> {
    match *inner {
        TypeInner::Scalar(scalar) => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            Ok(smallvec::smallvec![t])
        }
        TypeInner::Vector { size, scalar, .. } => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            let n = vector_size_usize(size);
            Ok(SmallVec::from_elem(t, n))
        }
        TypeInner::Matrix {
            columns,
            rows,
            scalar,
            ..
        } => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            let n = vector_size_usize(columns) * vector_size_usize(rows);
            Ok(SmallVec::from_elem(t, n))
        }
        TypeInner::Struct { ref members, .. } => {
            let mut out: IrTypeVec = SmallVec::new();
            for m in members {
                let mem_inner = &module.types[m.ty].inner;
                out.extend(naga_type_to_ir_types(module, mem_inner)?);
            }
            Ok(out)
        }
        _ => Err(LowerError::UnsupportedType(format!(
            "unsupported type for LPIR: {inner:?}"
        ))),
    }
}

/// Single scalar IR type; use [`naga_type_to_ir_types`] for vectors and matrices.
#[allow(
    dead_code,
    reason = "convenience for scalar-only call sites and future passes"
)]
pub(crate) fn naga_type_to_ir_type(
    module: &Module,
    inner: &TypeInner,
) -> Result<IrType, LowerError> {
    let tys = naga_type_to_ir_types(module, inner)?;
    if tys.len() != 1 {
        return Err(LowerError::UnsupportedType(String::from(
            "expected a single scalar IR type",
        )));
    }
    Ok(tys[0])
}

pub(crate) fn naga_type_width(inner: &TypeInner) -> usize {
    match *inner {
        TypeInner::Scalar(_) => 1,
        TypeInner::Vector { size, .. } => vector_size_usize(size),
        TypeInner::Matrix { columns, rows, .. } => {
            vector_size_usize(columns) * vector_size_usize(rows)
        }
        TypeInner::ValuePointer {
            size: Some(vec_size),
            ..
        } => vector_size_usize(vec_size),
        TypeInner::ValuePointer { size: None, .. } => 1,
        _ => 1,
    }
}

/// Flatten a fixed-size Naga array to one LPIR type per scalar element (row-major) — only for
/// [`coerce_assignment_vregs`] and other value-shape coercions. Parameter and return ABIs use
/// [`array_ty_pointer_arg_ir_type`] / [`func_return_ir_types_with_sret`] instead.
fn array_type_flat_components_for_value_coercions(
    module: &Module,
    array_ty: Handle<Type>,
) -> Result<Vec<IrType>, LowerError> {
    let (dimensions, leaf_ty, _) =
        crate::lower_array_multidim::flatten_array_type_shape(module, array_ty)?;
    let element_count = dimensions
        .iter()
        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
        .ok_or_else(|| {
            LowerError::Internal(String::from(
                "array_type_flat_components_for_value_coercions: count overflow",
            ))
        })?;
    let leaf_inner = &module.types[leaf_ty].inner;
    let leaf_tys = naga_type_to_ir_types(module, leaf_inner)?;
    let mut out = Vec::new();
    for _ in 0..element_count {
        for ty in leaf_tys.iter() {
            out.push(*ty);
        }
    }
    Ok(out)
}

/// Scalar/vector/matrix via [`naga_type_to_ir_types`]; fixed-size arrays flattened for coercion.
pub(crate) fn ir_types_for_naga_type(
    module: &Module,
    ty: Handle<Type>,
) -> Result<Vec<IrType>, LowerError> {
    let inner = &module.types[ty].inner;
    if matches!(inner, TypeInner::Array { .. }) {
        array_type_flat_components_for_value_coercions(module, ty)
    } else {
        Ok(naga_type_to_ir_types(module, inner)?.to_vec())
    }
}

/// LPIR formal type for a by-value Naga array parameter (M1+): one pointer, caller slot address.
pub(crate) fn array_ty_pointer_arg_ir_type(
    _module: &Module,
    _ty: Handle<Type>,
) -> Result<IrType, LowerError> {
    Ok(IrType::Pointer)
}

/// Type-level layout for an aggregate Naga type (array or struct), shared by
/// every "is this aggregate, and if so where do its parts live?" decision in
/// the frontend. Returns `None` for non-aggregates.
///
pub(crate) fn aggregate_layout(
    module: &Module,
    ty: Handle<Type>,
) -> Result<Option<AggregateLayout>, LowerError> {
    const R: LayoutRules = LayoutRules::Std430;
    match &module.types[ty].inner {
        TypeInner::Array { .. } => {
            let (dimensions, leaf_element_ty, leaf_stride) =
                crate::lower_array_multidim::flatten_array_type_shape(module, ty)?;
            let element_count = dimensions
                .iter()
                .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                .ok_or_else(|| {
                    LowerError::Internal(String::from("aggregate_layout: count overflow"))
                })?;
            let (total_size, align) =
                crate::lower_aggregate_layout::aggregate_size_and_align(module, ty)?;
            Ok(Some(AggregateLayout {
                kind: AggregateKind::Array {
                    dimensions,
                    leaf_element_ty,
                    leaf_stride,
                    element_count,
                },
                total_size,
                align,
            }))
        }
        TypeInner::Struct { members, .. } => {
            let mut out_members = Vec::with_capacity(members.len());
            let mut current_offset = 0u32;
            let mut max_align = 1u32;
            let lps_ty = crate::lower_aggregate_layout::naga_to_lps_type(module, ty)?;
            let lps_shared::LpsType::Struct {
                members: lps_members,
                ..
            } = &lps_ty
            else {
                return Err(LowerError::Internal(String::from(
                    "aggregate_layout: naga struct without LpsType::Struct",
                )));
            };
            for (i, m) in members.iter().enumerate() {
                let lps_member_ty = lps_members[i].ty.clone();
                let member_align = lps_shared::type_alignment(&lps_member_ty, R) as u32;
                let byte_offset = align_u32(current_offset, member_align);
                let ir_tys = match &module.types[m.ty].inner {
                    TypeInner::Scalar(_) | TypeInner::Vector { .. } | TypeInner::Matrix { .. } => {
                        naga_type_to_ir_types(module, &module.types[m.ty].inner)?
                    }
                    _ => SmallVec::new(),
                };
                out_members.push(MemberInfo {
                    byte_offset,
                    naga_ty: m.ty,
                    lps_ty: lps_member_ty.clone(),
                    ir_tys,
                });
                current_offset = byte_offset + lps_shared::type_size(&lps_member_ty, R) as u32;
                max_align = max_align.max(member_align);
            }
            let total_size = align_u32(current_offset, max_align);
            Ok(Some(AggregateLayout {
                kind: AggregateKind::Struct {
                    members: out_members,
                },
                total_size,
                align: max_align,
            }))
        }
        _ => Ok(None),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AggregateLayout {
    pub kind: AggregateKind,
    pub total_size: u32,
    pub align: u32,
}

impl AggregateLayout {
    pub(crate) fn struct_members(&self) -> Option<&[MemberInfo]> {
        match &self.kind {
            AggregateKind::Struct { members } => Some(members.as_slice()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AggregateKind {
    Array {
        dimensions: SmallVec<[u32; 4]>,
        leaf_element_ty: Handle<Type>,
        leaf_stride: u32,
        element_count: u32,
    },
    Struct {
        members: Vec<MemberInfo>,
    },
}

/// Return ABI for a function, including an optional sret buffer for aggregate (array) returns.
pub(crate) struct FuncReturnAbi {
    /// LPIR `return_types` for the [`lpir::FunctionBuilder`]. Empty when sret is set.
    pub returns: Vec<IrType>,
    /// `Some` when the return is lowered via a hidden sret pointer parameter.
    pub sret: Option<IrType>,
    /// Byte size of the sret buffer (the aggregate’s std430 size). Zero when not used.
    pub sret_size: u32,
}

pub(crate) fn func_return_ir_types_with_sret(
    module: &Module,
    ret_ty: Option<Handle<Type>>,
) -> Result<FuncReturnAbi, LowerError> {
    let Some(h) = ret_ty else {
        return Ok(FuncReturnAbi {
            returns: Vec::new(),
            sret: None,
            sret_size: 0,
        });
    };
    if let Some(layout) = aggregate_layout(module, h)? {
        return Ok(FuncReturnAbi {
            returns: Vec::new(),
            sret: Some(IrType::Pointer),
            sret_size: layout.total_size,
        });
    }
    let inner = &module.types[h].inner;
    let tys = naga_type_to_ir_types(module, inner)?;
    Ok(FuncReturnAbi {
        returns: tys.to_vec(),
        sret: None,
        sret_size: 0,
    })
}

pub(crate) fn type_handle_scalar_kind(
    module: &Module,
    ty: Handle<naga::Type>,
) -> Result<ScalarKind, LowerError> {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Vector { scalar, .. } | TypeInner::Matrix { scalar, .. } => Ok(scalar.kind),
        TypeInner::Array { base, .. } => type_handle_scalar_kind(module, *base),
        _ => Err(LowerError::UnsupportedType(String::from(
            "expected scalar, vector, matrix, or array type",
        ))),
    }
}

/// Result type of an expression as [`TypeInner`] (by value; synthesized for comparisons, `As`, etc.).
pub(crate) fn expr_type_inner(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<TypeInner, LowerError> {
    match &func.expressions[expr] {
        Expression::Literal(lit) => Ok(literal_type_inner(lit)),
        Expression::Constant(h) => Ok(module.types[module.constants[*h].ty].inner.clone()),
        Expression::ZeroValue(ty_h) => Ok(module.types[*ty_h].inner.clone()),
        Expression::Compose { ty, .. } => Ok(module.types[*ty].inner.clone()),
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| LowerError::Internal(String::from("bad argument index")))?;
            // Keep Pointer type for `inout`/`out` so `Load`/`expr_type_inner` see a pointer.
            Ok(module.types[arg.ty].inner.clone())
        }
        Expression::LocalVariable(lv) => Ok(TypeInner::Pointer {
            base: func.local_variables[*lv].ty,
            space: naga::AddressSpace::Function,
        }),
        Expression::GlobalVariable(gv) => {
            let gv_data = &module.global_variables[*gv];
            Ok(TypeInner::Pointer {
                base: gv_data.ty,
                space: gv_data.space,
            })
        }
        Expression::ArrayLength(_) => Ok(TypeInner::Scalar(Scalar {
            kind: ScalarKind::Uint,
            width: 4,
        })),
        Expression::Load { pointer } => match expr_type_inner(module, func, *pointer)? {
            TypeInner::Pointer { base, space: _ } => {
                if let TypeInner::Atomic(scalar) = module.types[base].inner {
                    Ok(TypeInner::Scalar(scalar))
                } else {
                    Ok(module.types[base].inner.clone())
                }
            }
            TypeInner::ValuePointer {
                size,
                scalar,
                space: _,
            } => Ok(match size {
                Some(size) => TypeInner::Vector { size, scalar },
                None => TypeInner::Scalar(scalar),
            }),
            // `AccessIndex` / `Access` on pointer-to-array is typed as the element value, not `ValuePointer`.
            // `Access` on pointer-to-array-of-struct yields [`TypeInner::Struct`]; `Load` reads that value.
            ty @ TypeInner::Scalar(_) => Ok(ty.clone()),
            ty @ TypeInner::Vector { .. } => Ok(ty.clone()),
            ty @ TypeInner::Matrix { .. } => Ok(ty.clone()),
            ty @ TypeInner::Struct { .. } => Ok(ty.clone()),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Load from non-pointer",
            ))),
        },
        Expression::Splat { size, value } => match expr_type_inner(module, func, *value)? {
            TypeInner::Scalar(scalar) => Ok(TypeInner::Vector {
                size: *size,
                scalar,
            }),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Splat of non-scalar",
            ))),
        },
        Expression::Swizzle { size, vector, .. } => match expr_type_inner(module, func, *vector)? {
            TypeInner::Vector { scalar, .. } => Ok(TypeInner::Vector {
                size: *size,
                scalar,
            }),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Swizzle of non-vector",
            ))),
        },
        Expression::AccessIndex { base, index } => {
            let base_ty = expr_type_inner(module, func, *base)?;
            match base_ty {
                TypeInner::Vector { size, scalar } => {
                    if *index >= vector_size_usize(size) as u32 {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex {index} out of bounds for vector"
                        )));
                    }
                    Ok(TypeInner::Scalar(scalar))
                }
                TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } => {
                    if *index >= vector_size_usize(columns) as u32 {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex {index} out of bounds for matrix columns"
                        )));
                    }
                    Ok(TypeInner::Vector { size: rows, scalar })
                }
                TypeInner::Struct { ref members, .. } => {
                    let idx = *index as usize;
                    let Some(member) = members.get(idx) else {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex struct index {index} out of range (len {})",
                            members.len()
                        )));
                    };
                    Ok(module.types[member.ty].inner.clone())
                }
                // `int a[2][3]; a[0]` → `AccessIndex` on nested array value (not a pointer).
                // NOTE: Allow index == size; values >= size are clamped at runtime.
                TypeInner::Array {
                    base: elt, size, ..
                } => {
                    let _in_bounds = match size {
                        ArraySize::Constant(nz) => *index <= nz.get(),
                        ArraySize::Pending(_) | ArraySize::Dynamic => true,
                    };
                    // We allow index == size for runtime clamp compatibility.
                    // The lowering will clamp with `min(index, size - 1)`.
                    Ok(module.types[elt].inner.clone())
                }
                TypeInner::Pointer { base, space } => match &module.types[base].inner {
                    TypeInner::Vector { size, scalar } => {
                        if *index >= vector_size_usize(*size) as u32 {
                            return Err(LowerError::UnsupportedExpression(format!(
                                "AccessIndex {index} out of bounds"
                            )));
                        }
                        Ok(TypeInner::ValuePointer {
                            size: None,
                            scalar: *scalar,
                            space,
                        })
                    }
                    TypeInner::Matrix {
                        columns,
                        rows,
                        scalar,
                    } => {
                        if *index >= vector_size_usize(*columns) as u32 {
                            return Err(LowerError::UnsupportedExpression(format!(
                                "AccessIndex {index} out of bounds for matrix"
                            )));
                        }
                        Ok(TypeInner::ValuePointer {
                            size: Some(*rows),
                            scalar: *scalar,
                            space,
                        })
                    }
                    TypeInner::Array { base: elt, .. } => Ok(module.types[*elt].inner.clone()),
                    TypeInner::Struct { members, .. } => {
                        let idx = *index as usize;
                        let Some(member) = members.get(idx) else {
                            return Err(LowerError::UnsupportedExpression(format!(
                                "AccessIndex struct index {index} out of range (len {})",
                                members.len()
                            )));
                        };
                        Ok(TypeInner::Pointer {
                            base: member.ty,
                            space,
                        })
                    }
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "AccessIndex base not vector/matrix/array",
                    ))),
                },
                // e.g. `t[0][1]`: `t[0]` is a column pointer (`ValuePointer` to vecN).
                TypeInner::ValuePointer {
                    size: Some(vec_size),
                    scalar,
                    space,
                } => {
                    if *index >= vector_size_usize(vec_size) as u32 {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex {index} out of bounds for value pointer vector"
                        )));
                    }
                    Ok(TypeInner::ValuePointer {
                        size: None,
                        scalar,
                        space,
                    })
                }
                other => Err(LowerError::UnsupportedExpression(format!(
                    "AccessIndex on unsupported base {other:?}"
                ))),
            }
        }
        // Same value shape as `AccessIndex`; index is dynamic (bounds checked at runtime if needed).
        Expression::Access { base, index: _ } => {
            let base_ty = expr_type_inner(module, func, *base)?;
            match base_ty {
                TypeInner::Vector { scalar, .. } => Ok(TypeInner::Scalar(scalar)),
                TypeInner::Matrix { rows, scalar, .. } => {
                    Ok(TypeInner::Vector { size: rows, scalar })
                }
                TypeInner::Array { base: elt, .. } => Ok(module.types[elt].inner.clone()),
                TypeInner::Pointer { base: ty_h, space } => match &module.types[ty_h].inner {
                    TypeInner::Vector { scalar, .. } => Ok(TypeInner::ValuePointer {
                        size: None,
                        scalar: *scalar,
                        space,
                    }),
                    TypeInner::Matrix { rows, scalar, .. } => Ok(TypeInner::ValuePointer {
                        size: Some(*rows),
                        scalar: *scalar,
                        space,
                    }),
                    TypeInner::Array { base: elt, .. } => Ok(module.types[*elt].inner.clone()),
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "Access base pointer not vector/matrix/array",
                    ))),
                },
                TypeInner::ValuePointer {
                    size: Some(_vec_size),
                    scalar,
                    space,
                } => Ok(TypeInner::ValuePointer {
                    size: None,
                    scalar,
                    space,
                }),
                other => Err(LowerError::UnsupportedExpression(format!(
                    "Access on unsupported base {other:?}"
                ))),
            }
        }
        Expression::Binary { op, left, .. } => match op {
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LogicalAnd
            | BinaryOperator::LogicalOr => {
                let li = expr_type_inner(module, func, *left)?;
                let bool_scalar = Scalar {
                    kind: ScalarKind::Bool,
                    width: 4,
                };
                match li {
                    TypeInner::Vector { size, .. } => Ok(TypeInner::Vector {
                        size,
                        scalar: bool_scalar,
                    }),
                    _ => Ok(TypeInner::Scalar(bool_scalar)),
                }
            }
            _ => expr_type_inner(module, func, *left),
        },
        Expression::Unary { expr: inner, .. } => expr_type_inner(module, func, *inner),
        Expression::Select { accept, .. } => expr_type_inner(module, func, *accept),
        Expression::As { kind, .. } => Ok(TypeInner::Scalar(Scalar {
            kind: *kind,
            width: 4,
        })),
        Expression::CallResult(fh) => {
            let ret = module.functions[*fh].result.as_ref().ok_or_else(|| {
                LowerError::UnsupportedExpression(String::from("CallResult for void function"))
            })?;
            Ok(module.types[ret.ty].inner.clone())
        }
        Expression::Math { fun, arg, arg1, .. } => {
            math_result_type_inner(module, func, *fun, *arg, *arg1)
        }
        Expression::Relational { fun, argument } => {
            relational_result_type_inner(module, func, *fun, *argument)
        }
        Expression::ImageLoad { .. } | Expression::ImageSample { .. } => Ok(TypeInner::Vector {
            size: VectorSize::Quad,
            scalar: Scalar {
                kind: ScalarKind::Float,
                width: 4,
            },
        }),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "expr_type_inner unsupported {:?}",
            func.expressions[expr]
        ))),
    }
}

/// Result type of `Expression::Relational` (`all`/`any` → scalar bool; `isnan`/`isinf` → bool vector).
fn relational_result_type_inner(
    module: &Module,
    func: &Function,
    fun: RelationalFunction,
    argument: Handle<Expression>,
) -> Result<TypeInner, LowerError> {
    let bool_scalar = Scalar {
        kind: ScalarKind::Bool,
        width: 4,
    };
    match fun {
        RelationalFunction::All | RelationalFunction::Any => Ok(TypeInner::Scalar(bool_scalar)),
        RelationalFunction::IsNan | RelationalFunction::IsInf => {
            let arg_ty = expr_type_inner(module, func, argument)?;
            match arg_ty {
                TypeInner::Vector { size, scalar } if scalar.kind == ScalarKind::Float => {
                    Ok(TypeInner::Vector {
                        size,
                        scalar: bool_scalar,
                    })
                }
                TypeInner::Scalar(s) if s.kind == ScalarKind::Float => {
                    Ok(TypeInner::Scalar(bool_scalar))
                }
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "isnan/isinf expect float scalar or vector",
                ))),
            }
        }
    }
}

fn literal_type_inner(lit: &Literal) -> TypeInner {
    let (kind, width) = match lit {
        Literal::F32(_) | Literal::F64(_) | Literal::F16(_) | Literal::AbstractFloat(_) => {
            (ScalarKind::Float, 4)
        }
        Literal::I32(_) | Literal::I64(_) | Literal::AbstractInt(_) => (ScalarKind::Sint, 4),
        Literal::U32(_) | Literal::U64(_) => (ScalarKind::Uint, 4),
        Literal::Bool(_) => (ScalarKind::Bool, 4),
    };
    TypeInner::Scalar(Scalar { kind, width })
}

fn math_result_type_inner(
    module: &Module,
    func: &Function,
    fun: MathFunction,
    arg: Handle<Expression>,
    arg1: Option<Handle<Expression>>,
) -> Result<TypeInner, LowerError> {
    let arg_ty = expr_type_inner(module, func, arg)?;
    match fun {
        MathFunction::Dot | MathFunction::Length | MathFunction::Distance => {
            Ok(TypeInner::Scalar(Scalar {
                kind: ScalarKind::Float,
                width: 4,
            }))
        }
        MathFunction::Cross => Ok(TypeInner::Vector {
            size: VectorSize::Tri,
            scalar: Scalar {
                kind: ScalarKind::Float,
                width: 4,
            },
        }),
        MathFunction::Transpose => {
            let TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } = arg_ty
            else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "transpose non-matrix",
                )));
            };
            Ok(TypeInner::Matrix {
                columns: rows,
                rows: columns,
                scalar,
            })
        }
        MathFunction::Determinant => Ok(TypeInner::Scalar(Scalar {
            kind: ScalarKind::Float,
            width: 4,
        })),
        MathFunction::Inverse => Ok(arg_ty),
        MathFunction::Outer => {
            let Some(a1) = arg1 else {
                return Err(LowerError::Internal(String::from("outer missing arg")));
            };
            let t0 = expr_type_inner(module, func, arg)?;
            let t1 = expr_type_inner(module, func, a1)?;
            let (s0, k0) = vector_kind_rows(&t0)?;
            let (s1, k1) = vector_kind_rows(&t1)?;
            if k0.kind != ScalarKind::Float || k1.kind != ScalarKind::Float {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "outer non-float",
                )));
            }
            Ok(TypeInner::Matrix {
                columns: s1,
                rows: s0,
                scalar: k0,
            })
        }
        _ => Ok(arg_ty),
    }
}

fn vector_kind_rows(inner: &TypeInner) -> Result<(VectorSize, Scalar), LowerError> {
    match *inner {
        TypeInner::Vector { size, scalar } => Ok((size, scalar)),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "expected vector for outer",
        ))),
    }
}

pub(crate) fn expr_scalar_kind(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<ScalarKind, LowerError> {
    match &func.expressions[expr] {
        Expression::Constant(h) => type_handle_scalar_kind(module, module.constants[*h].ty),
        Expression::Literal(l) => match l {
            Literal::F32(_) | Literal::F64(_) | Literal::F16(_) | Literal::AbstractFloat(_) => {
                Ok(ScalarKind::Float)
            }
            Literal::I32(_) | Literal::I64(_) | Literal::AbstractInt(_) => Ok(ScalarKind::Sint),
            Literal::U32(_) | Literal::U64(_) => Ok(ScalarKind::Uint),
            Literal::Bool(_) => Ok(ScalarKind::Bool),
        },
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| LowerError::Internal(String::from("bad argument index")))?;
            match &module.types[arg.ty].inner {
                TypeInner::Pointer { base, .. } => type_handle_scalar_kind(module, *base),
                _ => type_handle_scalar_kind(module, arg.ty),
            }
        }
        Expression::LocalVariable(lv) => {
            let lv_ty = func.local_variables[*lv].ty;
            type_handle_scalar_kind(module, lv_ty)
        }
        Expression::GlobalVariable(gv) => {
            let gv_ty = module.global_variables[*gv].ty;
            type_handle_scalar_kind(module, gv_ty)
        }
        Expression::Load { pointer } => match &func.expressions[*pointer] {
            Expression::LocalVariable(lv) => {
                let lv_ty = func.local_variables[*lv].ty;
                type_handle_scalar_kind(module, lv_ty)
            }
            _ => expr_scalar_kind(module, func, *pointer),
        },
        Expression::Compose { ty, .. } => type_handle_scalar_kind(module, *ty),
        Expression::Splat { value, .. } => expr_scalar_kind(module, func, *value),
        Expression::Swizzle { vector, .. } => expr_scalar_kind(module, func, *vector),
        Expression::AccessIndex { base, index } => {
            let base_ty = expr_type_inner(module, func, *base)?;
            match base_ty {
                TypeInner::Struct { members, .. } => {
                    let idx = *index as usize;
                    let Some(m) = members.get(idx) else {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex struct index {index} out of range (len {})",
                            members.len()
                        )));
                    };
                    type_handle_scalar_kind(module, m.ty)
                }
                TypeInner::Pointer {
                    base: pointee_h, ..
                } => match &module.types[pointee_h].inner {
                    TypeInner::Struct { members, .. } => {
                        let idx = *index as usize;
                        let Some(m) = members.get(idx) else {
                            return Err(LowerError::UnsupportedExpression(format!(
                                "AccessIndex struct index {index} out of range (len {})",
                                members.len()
                            )));
                        };
                        type_handle_scalar_kind(module, m.ty)
                    }
                    _ => expr_scalar_kind(module, func, *base),
                },
                _ => expr_scalar_kind(module, func, *base),
            }
        }
        Expression::Access { base, .. } => expr_scalar_kind(module, func, *base),
        Expression::Binary { op, left, .. } => match op {
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LogicalAnd
            | BinaryOperator::LogicalOr => Ok(ScalarKind::Bool),
            _ => expr_scalar_kind(module, func, *left),
        },
        Expression::Unary { expr: inner, .. } => expr_scalar_kind(module, func, *inner),
        Expression::Select { accept, .. } => expr_scalar_kind(module, func, *accept),
        Expression::As { kind, .. } => Ok(*kind),
        Expression::CallResult(fh) => {
            let ret = module.functions[*fh].result.as_ref().ok_or_else(|| {
                LowerError::UnsupportedExpression(String::from("CallResult for void function"))
            })?;
            type_handle_scalar_kind(module, ret.ty)
        }
        Expression::ZeroValue(ty_h) => type_handle_scalar_kind(module, *ty_h),
        Expression::Math { fun, arg, .. } => match fun {
            MathFunction::Dot
            | MathFunction::Length
            | MathFunction::Distance
            | MathFunction::Determinant => Ok(ScalarKind::Float),
            MathFunction::Cross => Ok(ScalarKind::Float),
            MathFunction::Transpose | MathFunction::Inverse => expr_scalar_kind(module, func, *arg),
            _ => expr_scalar_kind(module, func, *arg),
        },
        Expression::Relational { fun, .. } => match fun {
            RelationalFunction::All | RelationalFunction::Any => Ok(ScalarKind::Bool),
            RelationalFunction::IsNan | RelationalFunction::IsInf => Ok(ScalarKind::Bool),
        },
        Expression::ImageLoad { .. } | Expression::ImageSample { .. } => Ok(ScalarKind::Float),
        Expression::ArrayLength(_) => Ok(ScalarKind::Uint),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "cannot infer scalar kind for {:?}",
            func.expressions[expr]
        ))),
    }
}

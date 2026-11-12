//! The constant-expression rule for `const` initializers (GLSL ES 3.00
//! §4.3.3, GLSL 4.60 §4.3.3.1).

use crate::{Diagnostic, Span};

use super::arena::ExprId;
use super::type_table::HirView;
use super::types::HirExprKind;

/// Reject a typed `const` initializer that is not a constant expression:
/// one that reads a uniform, a global variable or a place, calls a user
/// function, samples a texture, or writes anything. Literals, constructors,
/// operators, swizzles and indexing of constants, built-in calls and other
/// global consts (already inlined as their typed expression) are constant.
pub(super) fn check_constant_expr(view: HirView<'_>, expr: ExprId) -> Result<(), Diagnostic> {
    match first_non_constant(view, expr) {
        None => Ok(()),
        Some((span, why)) => Err(Diagnostic::error(
            span,
            alloc::format!("not a constant expression: {why}"),
        )),
    }
}

fn first_non_constant(view: HirView<'_>, expr: ExprId) -> Option<(Span, &'static str)> {
    let node = view.expr(expr);
    let list = |list| {
        view.expr_list(list)
            .iter()
            .find_map(|arg| first_non_constant(view, *arg))
    };
    match node.kind {
        HirExprKind::BoolLiteral(_)
        | HirExprKind::FloatLiteral(_)
        | HirExprKind::IntLiteral(_)
        | HirExprKind::UIntLiteral(_) => None,
        HirExprKind::Constructor { args } => list(*args),
        HirExprKind::Cast { expr } | HirExprKind::Unary { expr, .. } => {
            first_non_constant(view, *expr)
        }
        HirExprKind::Swizzle { base, .. } => first_non_constant(view, *base),
        HirExprKind::Index { base, index } => {
            first_non_constant(view, *base).or_else(|| first_non_constant(view, *index))
        }
        HirExprKind::Binary { lhs, rhs, .. } => {
            first_non_constant(view, *lhs).or_else(|| first_non_constant(view, *rhs))
        }
        HirExprKind::Sequence { first, second } => {
            first_non_constant(view, *first).or_else(|| first_non_constant(view, *second))
        }
        HirExprKind::Conditional {
            condition,
            accept,
            reject,
        } => first_non_constant(view, *condition)
            .or_else(|| first_non_constant(view, *accept))
            .or_else(|| first_non_constant(view, *reject)),
        HirExprKind::Builtin {
            args, writebacks, ..
        } if view.writebacks(*writebacks).is_empty() => list(*args),
        HirExprKind::ImportCall {
            args, out: None, ..
        } => list(*args),
        HirExprKind::Builtin { .. } | HirExprKind::ImportCall { .. } => {
            Some((node.span, "a built-in call that writes an out argument"))
        }
        HirExprKind::UserCall { .. } => Some((node.span, "calls a user function")),
        HirExprKind::Uniform { .. } => Some((node.span, "reads a uniform")),
        HirExprKind::Global { .. } | HirExprKind::PlaceRead { .. } => {
            Some((node.span, "reads a non-const variable"))
        }
        HirExprKind::Param { .. } | HirExprKind::Local { .. } => {
            Some((node.span, "reads a variable"))
        }
        HirExprKind::TexelFetch { .. } | HirExprKind::Texture { .. } => {
            Some((node.span, "samples a texture"))
        }
        HirExprKind::Assign { .. } | HirExprKind::IncDec { .. } => {
            Some((node.span, "writes a variable"))
        }
    }
}

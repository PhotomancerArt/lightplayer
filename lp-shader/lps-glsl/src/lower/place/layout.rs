use crate::hir::{HirExpr, HirExprKind};

pub(super) fn constant_index(expr: &HirExpr) -> Option<usize> {
    match expr.kind {
        HirExprKind::IntLiteral(value) => usize::try_from(value).ok(),
        HirExprKind::UIntLiteral(value) => usize::try_from(value).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use lps_shared::LpsType;

    use super::*;
    use crate::Span;

    #[test]
    fn constant_index_rejects_negative_values() {
        let expr = HirExpr {
            span: Span::new(0, 1),
            ty: LpsType::Int,
            kind: HirExprKind::IntLiteral(-1),
        };

        assert_eq!(constant_index(&expr), None);
    }

    #[test]
    fn constant_index_accepts_uint_values() {
        let expr = HirExpr {
            span: Span::new(0, 1),
            ty: LpsType::UInt,
            kind: HirExprKind::UIntLiteral(3),
        };

        assert_eq!(constant_index(&expr), Some(3));
    }
}

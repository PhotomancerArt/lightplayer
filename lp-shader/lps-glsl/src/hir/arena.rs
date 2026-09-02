use alloc::vec::Vec;

use lp_collection::ChunkedVec;
use lps_shared::LpsType;

use crate::Span;

use super::place::HirPlace;
use super::types::{HirExpr, HirExprKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExprId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlaceId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExprList {
    start: u32,
    len: u16,
}

#[derive(Debug, Clone, Default)]
pub struct HirArena {
    exprs: ChunkedVec<HirExpr>,
    places: ChunkedVec<HirPlace>,
    expr_lists: Vec<ExprId>,
}

impl HirArena {
    pub(crate) fn push_expr(&mut self, span: Span, ty: LpsType, kind: HirExprKind) -> ExprId {
        let id = ExprId(
            self.exprs
                .len()
                .try_into()
                .expect("HIR expression arena exceeded u32"),
        );
        self.exprs.push(HirExpr { span, ty, kind });
        id
    }

    pub(crate) fn expr(&self, id: ExprId) -> &HirExpr {
        self.exprs
            .get(id.index())
            .expect("HIR expression id out of range")
    }

    pub(crate) fn expr_ty(&self, id: ExprId) -> &LpsType {
        &self.expr(id).ty
    }

    pub(crate) fn expr_span(&self, id: ExprId) -> Span {
        self.expr(id).span
    }

    pub(crate) fn push_place(&mut self, place: HirPlace) -> PlaceId {
        let id = PlaceId(
            self.places
                .len()
                .try_into()
                .expect("HIR place arena exceeded u32"),
        );
        self.places.push(place);
        id
    }

    pub(crate) fn place(&self, id: PlaceId) -> &HirPlace {
        self.places
            .get(id.index())
            .expect("HIR place id out of range")
    }

    pub(crate) fn push_expr_list<I>(&mut self, ids: I) -> ExprList
    where
        I: IntoIterator<Item = ExprId>,
    {
        let start = self.expr_lists.len();
        self.expr_lists.extend(ids);
        let len = self.expr_lists.len() - start;
        ExprList {
            start: start.try_into().expect("HIR expression list exceeded u32"),
            len: len.try_into().expect("HIR expression list exceeded u16"),
        }
    }

    pub(crate) fn expr_list(&self, list: ExprList) -> &[ExprId] {
        let start = list.start as usize;
        let end = start + usize::from(list.len);
        &self.expr_lists[start..end]
    }
}

impl ExprId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

impl PlaceId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

impl ExprList {
    pub fn len(self) -> usize {
        usize::from(self.len)
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod size_tests {
    extern crate std;
    /// Prints the HIR node sizes the memory probes reason with (`cargo test
    /// -p lps-glsl -- hir_node_sizes_print --nocapture`). No assertion: the
    /// numbers are host sizes (8-byte pointers) and feed the per-node
    /// arithmetic in the compile-peak investigation, not a contract.
    #[test]
    fn hir_node_sizes_print() {
        use core::mem::size_of;
        use lps_shared::{LpsType, StructMember};

        use super::super::place::{HirPlace, PlaceSegment};
        use super::super::types::{HirExpr, HirStmt};
        use super::*;
        use crate::Token;
        use crate::body::ParsedExpr;

        std::println!("LpsType        {:>4} B", size_of::<LpsType>());
        std::println!("StructMember   {:>4} B", size_of::<StructMember>());
        std::println!("HirExpr        {:>4} B", size_of::<HirExpr>());
        std::println!("HirPlace       {:>4} B", size_of::<HirPlace>());
        std::println!("PlaceSegment   {:>4} B", size_of::<PlaceSegment>());
        std::println!("HirStmt        {:>4} B", size_of::<HirStmt>());
        std::println!("Token          {:>4} B", size_of::<Token>());
        std::println!("ParsedExpr     {:>4} B", size_of::<ParsedExpr>());
        std::println!("ExprId/PlaceId {:>4} B", size_of::<ExprId>());
    }
}

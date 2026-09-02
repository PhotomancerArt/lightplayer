use alloc::vec::Vec;

use lp_collection::ChunkedVec;
use lps_shared::LpsType;

use crate::Span;

use super::place::{HirPlace, PlaceRecord, PlaceSegment, SegmentList};
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
    places: ChunkedVec<PlaceRecord>,
    expr_lists: Vec<ExprId>,
    /// Every place's segments, back to back; a [`PlaceRecord`] spans its
    /// own. One list instead of one `Vec` per place: a place has one or
    /// two segments and a `Vec` of them cost a 4-element allocation each.
    segments: Vec<PlaceSegment>,
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
        let start = self.segments.len();
        self.segments.extend(place.segments);
        let len = self.segments.len() - start;
        self.places.push(PlaceRecord {
            root: place.root,
            segments: SegmentList {
                start: start.try_into().expect("HIR segment list exceeded u32"),
                len: len.try_into().expect("HIR place exceeded u16 segments"),
            },
            ty: place.ty,
        });
        id
    }

    pub(crate) fn place(&self, id: PlaceId) -> &PlaceRecord {
        self.places
            .get(id.index())
            .expect("HIR place id out of range")
    }

    /// The projections of place `id`, root first.
    pub(crate) fn place_segments(&self, id: PlaceId) -> &[PlaceSegment] {
        let list = self.place(id).segments;
        let start = list.start as usize;
        &self.segments[start..start + usize::from(list.len)]
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

        use super::super::place::{HirPlace, PlaceRecord, PlaceSegment};
        use super::super::types::{HirExpr, HirStmt};
        use super::*;
        use crate::Token;
        use crate::body::ParsedExpr;

        std::println!("LpsType        {:>4} B", size_of::<LpsType>());
        std::println!("StructMember   {:>4} B", size_of::<StructMember>());
        std::println!("HirExpr        {:>4} B", size_of::<HirExpr>());
        std::println!("HirPlace       {:>4} B", size_of::<HirPlace>());
        std::println!("PlaceRecord    {:>4} B", size_of::<PlaceRecord>());
        std::println!("PlaceSegment   {:>4} B", size_of::<PlaceSegment>());
        std::println!("HirStmt        {:>4} B", size_of::<HirStmt>());
        std::println!("Token          {:>4} B", size_of::<Token>());
        std::println!("ParsedExpr     {:>4} B", size_of::<ParsedExpr>());
        std::println!("ExprId/PlaceId {:>4} B", size_of::<ExprId>());
    }
}

use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::Span;

use super::arena::{ExprId, ExprList, HirArena, PlaceId, WritebackList};
use super::place::{HirPlace, PlaceRef, PlaceRoot, PlaceSegment};
use super::types::{HirExprKind, HirExprRef, HirUserCallWriteback};

/// Index into a module's [`TypeTable`]. Valid across every arena of the
/// module: a function body, a global const's initialiser and a signature
/// all refer to the same entry for the same type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeId(u32);

/// The distinct types of a module's HIR, deduplicated by equality — the
/// one home a type has inside a compile. Every expression, place, local and
/// signature refers to it by [`TypeId`]; nothing in the HIR owns an
/// `LpsType`. Owned by the build state while functions are typed and by
/// [`HirModule`](super::HirModule) afterwards (ADR
/// `2026-09-02-glsl-module-wide-type-table`).
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    types: Vec<LpsType>,
}

impl TypeTable {
    /// The id of `ty` in the table, adding it if new.
    ///
    /// A linear scan with `==`: a module holds tens of distinct types, and
    /// scalar comparisons are a discriminant check. The caller's `ty` is
    /// dropped when a match exists, so a struct type cloned to build a node
    /// is transient rather than resident.
    pub(crate) fn intern(&mut self, ty: LpsType) -> TypeId {
        if let Some(index) = self.types.iter().position(|t| *t == ty) {
            return TypeId(index as u32);
        }
        let id = TypeId(
            self.types
                .len()
                .try_into()
                .expect("HIR type table exceeded u32"),
        );
        self.types.push(ty);
        id
    }

    /// The id of `ty` in the table, adding it if new.
    ///
    /// A linear scan with `==`: a module holds tens of distinct types, and
    /// scalar comparisons are a discriminant check. The caller's `ty` is
    /// dropped when a match exists, so a struct type cloned to build a node
    /// is transient rather than resident.
    pub(crate) fn ty(&self, id: TypeId) -> &LpsType {
        &self.types[id.0 as usize]
    }

    /// Number of distinct types interned so far.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

/// A function's arena read together with the module's type table: what
/// lowering and the const folders see. `Copy`, so a reader can hold a
/// resolved node across other borrows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HirView<'a> {
    pub(crate) arena: &'a HirArena,
    pub(crate) types: &'a TypeTable,
}

impl<'a> HirView<'a> {
    pub(crate) fn expr(self, id: ExprId) -> HirExprRef<'a> {
        let node = self.arena.node(id);
        HirExprRef {
            span: node.span,
            ty: self.types.ty(node.ty),
            kind: &node.kind,
        }
    }

    pub(crate) fn expr_ty(self, id: ExprId) -> &'a LpsType {
        self.types.ty(self.arena.expr_type_id(id))
    }

    pub(crate) fn expr_span(self, id: ExprId) -> Span {
        self.arena.expr_span(id)
    }

    pub(crate) fn expr_list(self, list: ExprList) -> &'a [ExprId] {
        self.arena.expr_list(list)
    }

    pub(crate) fn place(self, id: PlaceId) -> PlaceRef<'a> {
        let record = self.arena.place_record(id);
        PlaceRef {
            root: &record.root,
            ty: self.types.ty(record.ty),
        }
    }

    pub(crate) fn place_segments(self, id: PlaceId) -> &'a [PlaceSegment] {
        self.arena.place_segments(id)
    }

    pub(crate) fn writebacks(self, list: WritebackList) -> &'a [HirUserCallWriteback] {
        self.arena.writebacks(list)
    }
}

/// A function's arena under construction, with the module's type table
/// borrowed for interning: what typing writes through. Reads resolve types
/// the same way [`HirView`] does.
#[derive(Debug)]
pub(crate) struct TypedArena<'t> {
    pub(super) arena: HirArena,
    pub(super) types: &'t mut TypeTable,
}

impl<'t> TypedArena<'t> {
    pub(super) fn new(types: &'t mut TypeTable) -> Self {
        Self {
            arena: HirArena::default(),
            types,
        }
    }

    pub(crate) fn view(&self) -> HirView<'_> {
        HirView {
            arena: &self.arena,
            types: self.types,
        }
    }

    /// Hand the finished arena over (to a [`HirFunctionBody`] or a global
    /// const); the table stays with the module.
    pub(super) fn take_arena(&mut self) -> HirArena {
        core::mem::take(&mut self.arena)
    }

    pub(crate) fn ty(&self, id: TypeId) -> &LpsType {
        self.types.ty(id)
    }

    pub(crate) fn expr(&self, id: ExprId) -> HirExprRef<'_> {
        self.view().expr(id)
    }

    pub(crate) fn expr_ty(&self, id: ExprId) -> &LpsType {
        self.view().expr_ty(id)
    }

    pub(crate) fn expr_type_id(&self, id: ExprId) -> TypeId {
        self.arena.expr_type_id(id)
    }

    pub(crate) fn expr_span(&self, id: ExprId) -> Span {
        self.arena.expr_span(id)
    }

    pub(crate) fn expr_list(&self, list: ExprList) -> &[ExprId] {
        self.arena.expr_list(list)
    }

    pub(crate) fn place(&self, id: PlaceId) -> PlaceRef<'_> {
        self.view().place(id)
    }

    pub(crate) fn place_type_id(&self, id: PlaceId) -> TypeId {
        self.arena.place_type_id(id)
    }

    pub(crate) fn place_segments(&self, id: PlaceId) -> &[PlaceSegment] {
        self.arena.place_segments(id)
    }

    pub(crate) fn push_expr(&mut self, span: Span, ty: LpsType, kind: HirExprKind) -> ExprId {
        let ty = self.types.intern(ty);
        self.arena.push_expr_typed(span, ty, kind)
    }

    /// [`Self::push_expr`] with an already-interned type — the way to give
    /// a node the type of another node without cloning it.
    pub(crate) fn push_expr_typed(&mut self, span: Span, ty: TypeId, kind: HirExprKind) -> ExprId {
        self.arena.push_expr_typed(span, ty, kind)
    }

    pub(crate) fn push_place(&mut self, place: HirPlace) -> PlaceId {
        let ty = self.types.intern(place.ty);
        self.arena.push_place_typed(place.root, place.segments, ty)
    }

    /// [`Self::push_place`] with an already-interned type.
    pub(crate) fn push_place_typed<I>(
        &mut self,
        root: PlaceRoot,
        segments: I,
        ty: TypeId,
    ) -> PlaceId
    where
        I: IntoIterator<Item = PlaceSegment>,
    {
        self.arena.push_place_typed(root, segments, ty)
    }

    pub(crate) fn push_expr_list<I>(&mut self, ids: I) -> ExprList
    where
        I: IntoIterator<Item = ExprId>,
    {
        self.arena.push_expr_list(ids)
    }

    pub(crate) fn push_writebacks<I>(&mut self, items: I) -> WritebackList
    where
        I: IntoIterator<Item = HirUserCallWriteback>,
    {
        self.arena.push_writebacks(items)
    }
}

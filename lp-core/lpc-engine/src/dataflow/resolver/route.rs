//! How a query reaches its value — the part that only a structural change
//! can alter.
//!
//! Resolving a consumed slot or a bus channel is two jobs stacked together:
//! *decide* which binding answers it (owner depth, priority, merge policy —
//! all reads of the binding graph and of authored definitions), then *get*
//! the value through that decision (which may tick a producer, and must
//! happen every frame).
//!
//! Only the second job is per-frame. A [`ResolvedRoute`] is the first job's
//! answer, cached until the graph changes shape.

use alloc::vec::Vec;
use lpc_model::LpValue;

use crate::dataflow::binding::BindingRef;
use crate::dataflow::resolver::query_intern::QueryId;

/// Where a binding's value comes from, resolved down to something the frame
/// can act on without touching the binding graph again.
///
/// A binding that reads another slot or a bus channel is stored as the
/// [`QueryId`] of that query rather than as a `BindingSource`: the id is what
/// resolution needs, and deriving it means hashing a `SlotPath`. Ids and
/// routes share the same lifetime — both die when the graph changes — so the
/// id can simply be part of the decision.
#[derive(Clone, Debug)]
pub enum RouteTarget {
    /// A literal written into the binding; materialize it directly.
    Literal(LpValue),
    /// Resolve this query and adopt its value.
    Query(QueryId),
}

/// The decision about how one query is answered.
#[derive(Clone, Debug)]
pub enum ResolvedRoute {
    /// Resolve through this binding.
    Binding {
        binding_ref: BindingRef,
        target: RouteTarget,
    },
    /// No binding answers this query; the host produces it.
    Produce,
    /// A mergeable receiver: merge these sources by key, in this order.
    ///
    /// Bus providers are already expanded into leaves here — the recursive
    /// walk that flattens them reads only the binding graph, so it is part of
    /// the decision rather than part of the frame.
    MergeByKey {
        inputs: Vec<(BindingRef, RouteTarget)>,
    },
}

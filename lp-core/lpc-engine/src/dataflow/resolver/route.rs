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

use crate::dataflow::binding::{BindingRef, BindingSource};

/// The decision about how one query is answered.
#[derive(Clone, Debug)]
pub enum ResolvedRoute {
    /// Resolve through this binding's source.
    Binding {
        binding_ref: BindingRef,
        source: BindingSource,
    },
    /// No binding answers this query; the host produces it.
    Produce,
    /// A mergeable receiver: merge these sources by key, in this order.
    ///
    /// Bus providers are already expanded into leaves here — the recursive
    /// walk that flattens them reads only the binding graph, so it is part of
    /// the decision rather than part of the frame.
    MergeByKey {
        inputs: Vec<(BindingRef, BindingSource)>,
    },
}

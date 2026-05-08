use crate::node::kind::NodeKind;
use core::any::Any;

/// Authored body of a node artifact.
///
/// A `NodeDef` is source data: it is what a TOML artifact defines before the
/// engine instantiates a runtime node. This is deliberately not called
/// `Config`; a definition may eventually include config-like fields, params,
/// bindings, nested node invocations, and presentation metadata.
pub trait NodeDef: core::fmt::Debug {
    fn kind(&self) -> NodeKind;
    fn as_any(&self) -> &dyn Any;
}

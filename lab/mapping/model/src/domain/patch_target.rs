//! What patching is said of: the fixture, a component, or an object. Rules
//! are on the fixture and on components (`patch_rule.rs`); overrides on
//! objects (`object_override.rs`); problems name whichever it was.

use crate::domain::component::ComponentId;
use crate::domain::object::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatchTarget {
    /// The whole fixture: its Start, and how its top-level components
    /// follow each other.
    Fixture,
    /// A component: a path, a circle, a repeat, a group.
    Component(ComponentId),
    /// One object: a ring, a copy, or a path's own (in a copy).
    Object(ObjectId),
}

//! Static construction hook for compiled slot views.
//!
//! A slot view is a real Rust type that owns compiled [`SlotAccessor`] handles
//! for a slot root. Derive code wires the root type to its view type, while the
//! view itself stays discoverable in the filesystem.

use crate::{SlotAccessorError, SlotShapeRegistry};

/// Root type that can compile a typed, read-only slot view.
pub trait SlotViewRoot {
    type View;

    fn compile_slot_view(registry: &SlotShapeRegistry) -> Result<Self::View, SlotAccessorError>;
}

//! Static shape support for Rust-authored enum slots.

use crate::{SlotShape, StaticSlotShapeDescriptor};

/// Static shape for a Rust-authored enum slot.
///
/// This is intentionally smaller than enum access. It lets a record field embed
/// an enum shape while the enum's active variant and data access remain
/// hand-authored.
pub trait SlotEnumShape {
    const STATIC_SLOT_ENUM_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> = None;

    fn slot_enum_shape() -> SlotShape;
}

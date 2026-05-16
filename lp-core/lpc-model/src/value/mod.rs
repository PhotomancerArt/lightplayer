/// Legacy constraint model associated with [`legacy_kind`].
///
/// New slot-model code should attach constraints through typed slot leaves and
/// shape metadata instead of adding new uses here.
pub mod constraint;
pub mod legacy_kind;
pub mod lp_type;
pub mod lp_value;
pub mod value_path;

/// Compatibility path for the legacy quantity model.
///
/// New slot-model code should not add new dependencies on this module. Prefer
/// typed slot leaf descriptors whose semantic meaning owns its storage shape.
pub mod kind {
    pub use super::legacy_kind::*;
}

pub use crate::sync::with_revision::WithRevision;
pub use lp_type::{LpType, ModelEnumVariant, ModelStructMember};
pub use lp_value::LpValue;
pub use value_path::{PathParseError, Segment, ValuePath, parse_path};

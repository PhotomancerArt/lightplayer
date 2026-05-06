pub mod constraint;
pub mod legacy_kind;
pub mod model_type;
pub mod model_value;
pub mod value_path;

/// Compatibility path for the legacy quantity model.
///
/// New slot-model code should not add new dependencies on this module. Prefer
/// typed slot leaf descriptors whose semantic meaning owns its storage shape.
pub mod kind {
    pub use super::legacy_kind::*;
}

pub use crate::versioned::Versioned;
pub use model_type::{ModelStructMember, ModelType};
pub use model_value::ModelValue;
pub use value_path::{PathParseError, Segment, ValuePath, parse_path};

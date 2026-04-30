pub mod binding;
pub mod constraint;
pub mod kind;
mod prop;
pub mod prop_path;
pub mod shape;

pub use prop::Prop;
pub use prop_path::{PathParseError, PropPath, Segment, parse_path};

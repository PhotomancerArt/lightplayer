pub mod binding;
pub mod constraint;
pub mod kind;
pub mod prop_path;
mod prop_value;
pub mod shape;

pub use prop_path::{PathParseError, PropPath, Segment, parse_path};
pub use prop_value::PropValue;

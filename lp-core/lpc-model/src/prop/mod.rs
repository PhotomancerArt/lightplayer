pub mod binding;
pub mod constraint;
pub mod kind;
mod prop_value;
pub mod prop_path;
pub mod shape;

pub use prop_value::PropValue;
pub use prop_path::{PathParseError, PropPath, Segment, parse_path};

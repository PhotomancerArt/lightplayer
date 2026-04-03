//! GLSL ABI (Application Binary Interface) - types, values, layout, and paths.
//!
//! This crate provides everything needed to work with GLSL types at the ABI level:
//! - `GlslType`: the type enum (scalars, vectors, matrices, arrays, structs)
//! - `GlslValue`: tree representation of values
//! - `GlslData`: byte-buffer representation with layout rules
//! - Layout computation (std430)
//! - Path parsing and resolution

#![no_std]

extern crate alloc;

mod data;
mod data_error;
mod layout;
mod metadata;
mod path;
mod path_resolve;
mod value;
mod value_path;

pub use data::GlslData;
pub use data_error::GlslDataError;
pub use layout::{array_stride, round_up, type_alignment, type_size};
pub use metadata::{
    GlslFunctionMeta, GlslModuleMeta, GlslParamMeta, GlslParamQualifier, GlslType, LayoutRules,
    StructMember,
};
pub use path::{PathParseError, PathSegment, parse_path};
pub use path_resolve::PathError;
pub use value::GlslValue;
pub use value_path::GlslValuePathError;

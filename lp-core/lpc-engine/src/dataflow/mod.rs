//! Runtime dataflow machinery: bindings, bus channels, and demand resolution.
//!
//! Products and resources are adjacent value/lifecycle concepts. This module is
//! specifically the routing and resolution layer that decides where a requested
//! slot value comes from during engine execution.

pub mod binding;
pub mod bus;
pub mod resolver;

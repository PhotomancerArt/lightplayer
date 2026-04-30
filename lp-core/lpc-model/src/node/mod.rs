//! **Runtime** graph nodes: the live counterpart to authored path and spec
//! types in [`crate::types`].
//!
//! A [`NodeProps`] is an **object-safe** interface implemented by every concrete
//! on-graph object (see tests holding `Box<dyn NodeProps>`). It combines a cheap
//! [`NodeId`] with a stable [`TreePath`] and [`PropPath`]-keyed property access
//! over [`LpsValue`][`crate::LpsValue`] (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` trait surface, `00-design` Node sketch).

pub mod node_config;
pub mod node_id;
pub mod node_name;
pub mod node_prop_spec;
pub mod node_props;
pub mod node_spec;

pub use crate::tree::tree_path::TreePath;
pub use node_config::NodeConfig;
pub use node_id::NodeId;
pub use node_name::{NodeName, NodeNameError};
pub use node_spec::NodeSpec;

//! **Identity and addressing** types for the domain layer: runtime identity,
//! human-readable names, node paths, value paths, and bus channel names.
//!
//! These are separate from value and slot shape models; they describe how
//! authored graphs, runtime nodes, and the bus are named.
//! [`NodeId`] is a cheap process-local handle; strings like [`NodeName`] and [`NodePath`]
//! are the stable authored-addressing story.
//!
//! Implementation files: [`crate::node`], [`crate::value`], [`crate::bus`], [`crate::artifact`].

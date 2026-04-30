//! **Identity and addressing** types for the domain layer: runtime identity,
//! human-readable names, node paths, property paths, and bus channel names.
//!
//! These are separate from the Quantity model (`Kind`, `Shape`, …) but are how
//! authored graphs, runtime nodes, and the bus are *named* in `docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` and
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md`.
//! [`NodeId`] is a cheap process-local handle; strings like [`NodeName`] and [`NodePath`]
//! are the stable authored-addressing story.
//!
//! Implementation files: [`crate::node`], [`crate::prop`], [`crate::bus`], [`crate::artifact`].

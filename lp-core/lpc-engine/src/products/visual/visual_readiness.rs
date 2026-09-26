//! Whether a visual producer is rendering its real frame yet.
//!
//! A shader that has just been attached renders black until its first
//! compile: the first render only asks for a compile window, and the compile
//! runs in the next tick's window
//! (`docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`). A
//! consumer that must not show that black — the playlist holding its last
//! frame across a switch — asks this after sampling the product, instead of
//! guessing by frame count.

use alloc::string::String;

/// The answer to "is this product's latest render its real output?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisualReadiness {
    /// The producer rendered real output (a compiled program, or a producer
    /// that needs no compile).
    Ready,
    /// The producer rendered a placeholder (black) and will render for real
    /// later: a first compile is still waiting for its window.
    Pending,
    /// The producer will not render for real without an edit: its first
    /// compile failed, or the recovery ledger denied it. The reason is the
    /// producer's own.
    Failed(String),
}

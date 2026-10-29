//! The tree pane: one row per node, port and component.

use crate::domain::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct UiTree {
    /// The project's name.
    pub title: String,
    /// What to say when there's nothing in it.
    pub empty: Option<String>,
    pub rows: Vec<UiTreeRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiTreeRow {
    pub target: Target,
    pub depth: usize,
    pub label: String,
    /// `25 lamps`, shown on hover.
    pub detail: String,
    /// Made by its part, not authored: drawn lighter.
    pub derived: bool,
    /// A fixture's problems, while it has any: "2 problems", drawn loud
    /// (a mark and the words, never colour alone).
    pub problems: Option<String>,
    /// `fixture`, `port`; empty for what's drawn.
    pub kind: &'static str,
    pub selected: bool,
    /// A selected lamp lights its path or ring's row.
    pub lit: bool,
    /// The level the selection is in.
    pub context: bool,
}

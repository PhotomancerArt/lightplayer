//! A node's header: what it's called and what kind of thing it is.

#[derive(Debug, Clone, PartialEq)]
pub struct UiNodeHeader {
    pub name: String,
    /// What the user calls this kind of node: `fixture`, `shader`.
    pub kind: String,
    /// Pinned where it was dropped (shown only while pinning is on).
    pub pinned: bool,
    /// A fixture's problems, while it has any: "2 problems", in its title
    /// tab, loud (a mark and the words, never colour alone).
    pub problems: Option<String>,
    /// A frame's title tab, in screen pixels: wide enough for its name and
    /// the pin mark. It is also what a click hits (`area.rs`).
    pub title_px: f64,
}

//! A node the lab doesn't simulate: a clock, a power button, a playlist…
//! (fit-check §3, "other node kinds"). Shaders are render sources
//! (`render_source.rs`).
//!
//! A real project has them, and the canvas is the whole graph (D36), so
//! they are on it — a plain card with its kind, which you can select,
//! move and delete like any node, but nothing inside it is edited here.
//! Whatever reads and writes the project's files keeps the node's own
//! bytes; the model only knows it is there and what kind it is.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherNode {
    /// The product's kind, as written: `Clock`, `PowerButton`.
    pub kind: String,
}

impl OtherNode {
    pub fn new(kind: impl Into<String>) -> Self {
        Self { kind: kind.into() }
    }

    /// The kind in words for a card: `PowerButton` → `power button`.
    pub fn kind_words(&self) -> String {
        kind_words(&self.kind)
    }
}

/// A product kind in words: `PowerButton` → `power button`.
pub fn kind_words(kind: &str) -> String {
    let mut out = String::new();
    for (i, c) in kind.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.extend(c.to_lowercase());
    }
    out
}

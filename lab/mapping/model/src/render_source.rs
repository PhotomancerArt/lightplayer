//! A render source: a node that makes a picture — in the product a
//! shader, or another node with a visual product (a texture, a fluid).
//!
//! It writes its render product to a visual channel (`visual.out`), and
//! every fixture reading that channel samples it at its lamps (D55). The
//! lab does not run GLSL: what it shows is a stand-in [`Pattern`], picked
//! from the node. A shader opened from a real project keeps its source
//! text, to show, and is saved back byte for byte — the lab never edits
//! one.

use crate::channel::Channel;
use crate::pattern::Pattern;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSource {
    /// The product's kind, as written: `Shader`, `Texture`, `Fluid`.
    pub kind: String,
    /// A shader's GLSL, when the lab read it. Shown, never run.
    pub glsl: Option<String>,
    /// The visual channel it writes its picture to.
    pub writes: Channel,
    /// What the lab shows instead of running it.
    pub pattern: Pattern,
}

impl RenderSource {
    /// A shader writing `visual.out`, showing `pattern`.
    pub fn shader(pattern: Pattern) -> Self {
        Self {
            kind: "Shader".into(),
            glsl: None,
            writes: Channel::visual_out(),
            pattern,
        }
    }

    /// The kind in words for a card: `Shader` → `shader`.
    pub fn kind_words(&self) -> String {
        crate::other_node::kind_words(&self.kind)
    }
}
